# Next algorithmic improvements to implement

*  IMPLEMENTED, benefit not established (2026-07-12): a greedy, cost-estimate-driven anchor pass
   (`ASTMappingReason::GreedyAnchorBlock`, `src/diff/solve_greedy_anchor_blocks.rs`), requested to
   fill a real gap - every other container-pairing heuristic keys off identity (a shared name, arm
   signatures, a Dice coefficient over already-matched descendants), so an anonymous container
   (an `if` body, a loop body, a function body with no already-matched children) with no such
   anchor falls through to the final APTED pass unassisted. Estimates the cost of matching a
   candidate pair via a fast weighted longest-common-subsequence alignment over *direct* children
   only (each child is an opaque token, equal only on identical full-subtree hash; a matched pair
   costs 0, everything else costs the full subtree size of whichever child didn't survive - a
   `sequence_edit_cost` DP, not a real tree edit distance, which is what keeps it cheap enough to
   try on many more candidate pairs than APTED itself could afford). Pairs scoring at or under
   `MAX_COST_RATIO` (cost / combined subtree size) are assigned greedily, cheapest first,
   one-to-one within their positional group (see below). Wired in right before the final APTED call.
   **First two attempts, both reverted before this one landed:** (1) considering *every*
   still-unmatched node with >= 2 children and subtree size >= 4 a candidate regressed 9 fixtures by
   up to +53 mismatches each (0 improved anywhere), because `sequence_edit_cost` only looks at a
   pair's own direct children with no notion of surrounding context - two entirely unrelated
   `call_expression`s (one in a `for` loop condition, one in a `return` statement) matched because
   their `argument_list` happened to hash-identical by coincidence. (2) restricting candidates to
   genuine statement-sequence containers (`nodes::is_block_container`: `block`/`compound_statement`/
   `statement_block` per language, plus `flow_control_family`'s `if`/`match`/`switch`) cut that to 1
   regressed fixture (`javascript-fix-promises`, +4), still 0 improved anywhere - and sweeping
   `MAX_COST_RATIO` from 0.5 to 0.2 produced an **identical** result, proving the regression wasn't
   threshold-tunable: a byte-identical `statement_block` the human mapping relocates into a newly-
   inserted `try_statement` wrapper scored a *near-zero* cost ratio (cheapest possible match) because
   content-only scoring has no way to tell "same content, same place" from "same content, moved".
   **The fix that actually worked:** gate every candidate pair on a *positional* signal before cost
   is even consulted, per a user suggestion ("what if the positional anchor was the path of the
   nodes"). `positional_key_before`/`positional_key_after` walk each candidate up to its nearest
   already-matched ancestor (via `ASTMetadata::node_to_parent`) and record the kind of every node
   passed along the way (falling back to the full path from the file root if nothing above is
   matched yet); two candidates are only ever compared if that walk lands on a *corresponding*
   ancestor pair *and* the kind-path from that ancestor down to each candidate is identical. This
   directly kills both regressions: the two unrelated `call_expression`s have unrelated ancestor
   paths, so they're never compared; the relocated `statement_block`'s after-side path gains an
   extra `try_statement` segment the before-side path doesn't have, so the pair is rejected
   regardless of its cost score. Result at `MAX_COST_RATIO = 0.5`: **0 changed fixtures** (exact
   742/0 baseline match, verified against a saved pre-change CSV), while still firing 24 times
   across the 40-fixture corpus (`GreedyAnchor` column in `benchmark_optimal_solutions --csv`).
   Verified deterministic across two independent `--release` process runs (byte-identical CSVs) -
   group-processing order is explicitly sorted by `preorder_index` (not left to `HashMap` iteration
   order) for exactly this reason, since group resolution order can affect which group claims a
   shared descendant first; see the module's doc comment.
   Same situation as `BottomUpExpansion` below: implemented, correct, verified safe (zero
   regressions, deterministic), but zero measured benefit on the current fixture corpus - whether
   it's worth keeping in the pipeline is a call for whoever picks this up next. Unlike
   `BottomUpExpansion`, this fires on genuinely different content (anonymous containers with no
   already-matched children at all), so it may earn its keep on fixtures/languages not in the
   current corpus even without moving today's TOTAL.

*  IMPLEMENTED, threshold tuned, benefit not established (2026-07-11): bottom-up heuristic that
   detects nodes whose descendants are already mapped to each other and matches those nodes too,
   via `ASTMappingReason::BottomUpExpansion` (`src/diff/solve_bottom_up_expansion.rs`), gated by a
   Dice coefficient over full subtrees (a direct-children ratio was tried first and rejected - see
   that file's doc comment). Wired into `Diff::from_code` at a single, deliberately late call site
   (right before Pass 3's orphan blanket-delete/insert) after an earlier "after every top-down
   heuristic" placement regressed 4 `optimal_solutions` fixtures by letting a plausible-but-wrong
   candidate preempt a later, more precise pass.
   `DICE_THRESHOLD` was then swept from 0.5 to 0.95 against `benchmark_optimal_solutions`: 0.8-0.95
   all tie the 742/0 baseline exactly (identical mismatch count on every fixture); 0.78 and below
   start regressing (0.78 -> 746, 0.75 -> 749, 0.5 -> 826), and those regressions are real content
   mismatches (`identifier`/`scoped_identifier`/`field_initializer`, a `statement_block` matched to
   the wrong arrow function) - not generic/punctuation-token ties (`}`, `)`) that would suggest an
   equally-valid alternate optimal solution worth flagging for human review instead of reverting.
   No throughput difference was measurable at any threshold either (one release-build, single-run
   comparison at 0.85 vs. the pass disabled: within noise). Landed at 0.9 - it ties every other safe
   value on outcome while keeping the largest margin from the ~0.79 regression cliff, and there's no
   evidence a lower value buys anything to justify sitting closer to that cliff.
   Same situation as `identical-statement-runs` in the memory log: implemented, correct, and now
   tuned, but whether it's worth keeping in the pipeline at all is still a call for whoever picks
   this up next - it has fired ~29 times across the fixture corpus without net effect on either
   accuracy or measured speed.
*  Use the values more. At the moment, the node values are used in a all-or-nothing match. But we
   could also use the value similarity to compute the cost, so that identifiers that look more alike
   are cheaper to match in APTED.
   TRIED AND REVERTED (2026-07-11), container-dissimilarity-surcharge variant: `UnitCostModel::ren`
   currently charges 0 to match two same-kind *internal* nodes unconditionally, with the real cost
   of reuse-vs-replace left entirely to the children's recursive edit cost. That unconditional 0 has
   a side effect: it always waives exactly the root's own delete+insert cost (COST_DELETE +
   COST_INSERT = 2), so two same-kind containers are always >=2 cheaper to "match" than to replace
   wholesale, no matter how unrelated their content actually is - this looked like the mechanism
   behind the `rust-algorithm-change`/`kotlin-remove-function` gaps below (pure unit-cost prefers
   reuse the human doesn't want).
   Implemented a quantized-tier surcharge on that branch, reusing `leaf_texts_similar`'s character-
   bigram Dice metric (extracted into `nodes::text_similarity`, continuous 0.0-1.0) applied to the
   *whole subtree's* text (available for free - `ASTNodeMetadata.text` is a full-span
   `utf8_text()`, not leaf-only, despite the field doc saying "for leaf nodes"): similarity >= 0.6
   -> 0 (unchanged), >= 0.3 -> 1, below that -> 2 (cancels the subsidy, capped so it never
   *penalizes* matching relative to delete+insert). Capped at 500 chars of subtree text to keep the
   DP's inner loop cheap.
   Result: `kotlin-remove-function` and `rust-algorithm-change` - the two fixtures this was built
   for - moved by exactly 0 mismatches each. Root cause in hindsight: "near-duplicate" is the whole
   problem description for both gaps - two siblings that read as textually *very similar* to a
   human, which is exactly what character-bigram Dice also scores highly (>= 0.6), so the surcharge
   never engages for the case it targets. Text similarity cannot distinguish "same entity, edited"
   from "different-but-near-identical entity" - by construction they look the same to that metric,
   so no threshold value here can ever separate them.
   Meanwhile it broke 5 *previously-perfect* (0-mismatch) fixtures - `rust-data-structure` (0->9),
   `kotlin-refactor-function` (0->5), `python-refactoring` (0->5), `kotlin-add-data-class` (0->2),
   `javascript-refactor-arrow-func` (0->1) - plus regressed 2 already-imperfect ones
   (`typescript-async-await` +10, `cpp-ladybird-refactor-variables-if-changes` +6). Checked via
   `--details` per the standing "check for punctuation-tie false-regressions before reverting"
   policy: all real content mismatches (`identifier`, `token_tree`, `user_type`) - not `}`/`)` ties.
   Root cause: small containers with several internally-differing identifiers (e.g. a macro
   `token_tree`, a struct's field list) have low *aggregate* text similarity even when they're the
   correct match - the surcharge punished exactly the kind of legitimate reuse the existing
   per-child recursive cost was already handling correctly. TOTAL mismatches did drop 742 -> 739,
   but that's 4 already-broken fixtures improving by more than 5 clean ones broke - not a trade
   worth taking, especially against the target case's 0/0 result.
   Reverted in full (`UnitCostModel::ren`'s internal-node branch, `nodes::text_similarity`
   extraction). Whoever picks up "use the values more" next: subtree-text similarity is the wrong
   signal for the reuse-vs-replace question specifically because it's blind to exactly the
   distinction that matters (similar-looking-but-distinct vs. actually-the-same-thing-edited) -
   this needs either a different signal entirely (e.g. positional/identity context: is there a
   *closer* candidate elsewhere that a plain nearest-text-match would find instead?) or accepting
   this class of gap per the three options already listed under `rust-algorithm-change` below. The
   leaf-level idea (graduate `COST_UPDATE` itself by identifier similarity, rather than internal-
   node `ren`) is untested and may still be worth trying - it wasn't what this attempt built, and
   doesn't have the same "can't distinguish near-duplicate from renamed" problem since a leaf
   rename *is* exactly the "same entity, edited" case by construction.

# Next features to implement

*  Add "Diff script" generation that can take the ASTDiff and make a "insert, move, update, delete"
   script out of it.

# TUI follow-ups

*  Mouse support and bracketed paste handling in the TUI.
*  Re-review TUI suspend/resume (Ctrl-Z) behavior, not touched since the async event loop rewrite.
*  Headless mode (`--headless`) is still unimplemented.
*  Revisit the `Update` diff color (currently magenta) once seen against more real diffs.

# Possible code health improvements

*  Make code.rs parse code in the from_string if possible, and then remove parsing from diff_code
   diff.rs

## Code reuse / readability review (2026-07-12) - FIXED 2026-07-12

Full-codebase review focused on reuse and readability (not correctness/perf), then implemented the
same day. Verification throughout: `cargo test` (365/365 passing after every change) plus a
benchmark quality-gate - `benchmark_optimal_solutions --csv` columns 1-8 (`mismatches`,
`mismatch_pct`, `total_nodes`, `human_unsolved`, `algorithm_cost`, `human_cost`, `cost_diff`)
diffed byte-for-byte against a pre-change baseline after every risky change. Note: columns 9+
(which pass gets *credited* for a match) have pre-existing, harmless run-to-run jitter on 2/40
fixtures (kotlin-nextcloud-a-few-small-removals, rust-sniffnet-protocol) unrelated to this work -
don't mistake that for a regression if re-verifying later. Final gate after all changes: clean,
0 fixtures diverged.

**Collapsed:**
*  `ForestDist`/`DeltaTable`/`StrategyTable`/`Mat` -> one generic `Grid<T>` (`common.rs`).
   `ForestDist`/`Mat` are now pure type aliases (`Grid<u64>`/`Grid<i64>`, zero-cost);
   `DeltaTable`/`StrategyTable` wrap `Grid` and keep their own `get`/`set` (the former's `UNSET`
   sentinel logic is real behavior, not boilerplate, so it stays a wrapper not an alias).
*  `collect_before_subtree_targets`/`collect_after_subtree_targets` -> shared recursive
   `collect_subtree_targets` parameterized by a per-node classifier closure
   (`SubtreeTargetOutcome`), in `common.rs`.
*  `add_delete_mappings`/`add_insert_mappings` -> shared `add_prune_mappings`, parameterized over
   the four things that actually differ (node map, mapping-key shape, operation, cost fn).
*  `filter_before_nodes`/`filter_after_nodes` -> `filter_mapped_nodes(node_ids, node_map)`.
*  `common.rs`'s ~1760-line `#[cfg(test)] mod tests` -> split into `common/tests.rs` (pure move,
   zero behavior change; cut common.rs from 4143 to ~2380 lines).
*  The 9x hand-rolled preorder-DFS stack-walk: merged the two pairs that were provably identical
   (not just similar) - `solve_comment_nodes`/`solve_identical_diagnostic_statements`'s lockstep
   two-tree walk -> `nodes::map_identical_descendants`; `code/metadata.rs`'s
   `discover_reference_nodes`/`discover_semantic_structure_nodes` -> `metadata::walk_preorder`
   (order-independent for both, verified: one sorts its output afterward, the other keys a map by
   a type that can only occur once). Left `hash_tree_matching` (already shared, has its own
   `classify` closure), `add_identical_subtree` (metadata-based, recursive, no already-mapped
   check - a different shape, not just a differently-named copy), `compute_subtree_sizes`
   (needs real post-order), and `compute_node_info` (needs a true preorder index, reverse-pushes
   children) alone - each is a genuinely different traversal shape, not cosmetic duplication.
*  `collect_unmatched_containers`/`collect_unmatched_diagnostic_statements` -> `nodes::collect_unmatched`.
*  The `apted::for_nodes` + conditional-relabel idiom -> `nodes::anchor_pair_via_apted`.
*  `sample_repository` in `sample_test_diffs.rs`/`sample_code_pairs.rs` -> shared
   `stats::git::walk_single_parent_commit_diffs` (revwalk/commit-filter/diff machinery only; each
   caller still does its own delta filtering, since that genuinely differs).
*  Blob-size/UTF-8 validation -> `stats::git::text_len_if_in_range`.
*  The two hand-maintained `ASTMappingReason -> label` matches -> `ASTMappingReason::bucket_label`
   in `src/diff.rs`, called by both binaries (`benchmark_optimal_solutions.rs` still special-cases
   `APTED` locally, since its per-provenance-column behavior is a deliberate divergence, not drift).
*  `ascii_visualizer.rs::get_ast()`'s redundant reparse -> uses `code.ast` directly.
*  `stats.rs`'s `count_nodes` + `visit_for_kind_stats` double traversal in `expand_from_code` ->
   `compute_kind_stats` now returns the node count alongside the map (`count_nodes` itself is kept,
   still used by `benchmark_diff_pairs.rs`).
*  `stats.rs::for_path`'s 4-level nested match -> flattened with early returns.
*  TUI dialog list-navigation/render duplication (`theme_dialog.rs`/`file_dialog.rs`) ->
   `tui::components::move_selection` + `render_list_dialog`.
*  `scroll_to_cursor`/`scroll_to_show_row` (components/code_viewer.rs) -> the former now just
   calls the latter.
*  Dead code removed: `CodeViewerWidget::with_path/with_title/with_theme/with_syntax_highlighting`,
   `CodeViewer::widget_mut/widget/state_mut` (kept `state()` - it's actually used by
   `diff_viewer.rs`'s tests, the original review's claim there was wrong, caught by grepping the
   whole tree before deleting). Stale `#[allow(dead_code)]` comment on `hash_tree_matching::solve`
   removed (it's actively called).
*  Six near-copies of `find_first`/`first_child_of_kind` (all confined to `#[cfg(test)]`, one had
   different self-inclusion semantics than the other five) -> one `test::helper::find_first_of_kind`.

**Deliberately left, with why:**
*  `emit_before_subtree`/`emit_after_subtree` (common.rs) - assessed for the `Side`
   trait/enum collapse and rejected: ~10 orthogonal divergence points (decision-map type,
   `has_match_below` field, node map, mapping-key shape `(id,0)`/`(0,id)`, operation, cost fn, and
   a cross-call into the shared, *not* duplicated `emit_match` with side-dependent argument order)
   threaded through mutual recursion. Every design attempted (trait with ~10 methods, generic
   function with ~11 closure params passed through every recursive call) was harder to read and
   verify than the current ~40-line mirror pair - fails the basic "abstraction should cost less
   than the duplication" test. A future attempt should feel free to revisit if a cleaner
   decomposition presents itself, but forcing today's designs in would have made this the exact
   kind of code a transcription bug hides in.
*  `before_match_target`/`after_match_target`, and the `before_has_match_below`/
   `after_has_match_below` loop pair inside `resolve_forest` - genuinely tiny; a `Side`
   trait/enum here would cost more lines than it saves. Left as-is.
*  `compute_opt_strategy_post_l`/`compute_opt_strategy_post_r`, `spf_a`'s cost-closures,
   `resolve_forest`'s early-exit/dispatch/emission split, and the 5x inline `UnitCostModel`
   reconstruction - not attempted this pass (time-boxed to the higher-value items above); still
   worth doing, none looked unusually risky.
*  `CodeViewerState::set_cursor` clamping setter - not a safe reuse cleanup on inspection:
   `line_len`/`line_count` (needed for real content-aware clamping) live on `CodeViewerWidget`, not
   `CodeViewerState`, so a real invariant-enforcing setter needs a design decision (pass the widget
   in, or duplicate content-awareness into state), not a mechanical extraction.
*  `solve_structurally_identical_trees::solve_with_config` - still has zero callers, but its own
   doc comment says that's deliberate (kept for experimentation); left alone per that comment.

**Verification gaps to be aware of:** the TUI changes (dialogs, `scroll_to_cursor`) were verified
by `cargo test` (including the dialogs' own key-handling tests) and a clean compile, but not by
interactively driving the TUI - no visual/rendering regression check was done. The `sample_*`
binaries' refactor was verified by their own unit tests (which exercise `sample_repository` against
a real git fixture) plus a clean compile, not by a manual run against a real large repo.

# Diff algorithm accuracy (optimal_solutions gaps)

## Known gaps with full analysis

*  Premature/irreversible pruning in `solve_semantically_structural_nodes`'s Pass 3: when a
   name-keyed anchor (`impl_item`/`function_item` matched by identifier) fails to find a
   counterpart, Pass 3 immediately marks the whole subtree deleted/inserted via an O(n) shortcut,
   before the final full-tree APTED pass (which runs afterward and might find a real
   correspondence) ever gets a chance - `for_roots`/`resolve_forest` filters out anything already
   in `before_node_map`/`after_node_map`, so the decision is irrevocable. Surfaced by the
   `rust-turbopack-module-rule` optimal_solutions test: `impl ModuleType` was renamed to
   `impl ConfiguredModuleType` and its `from_str_with_defaults` method split into `parse` +
   `into_effect`, which share a near-identical parameter list with the original - but since the
   type name changed, they're never even considered for matching, and the whole impl body (234
   node mismatches worth) round-trips through delete+insert instead. Candidate directions: make
   the orphan handling advisory (run it after the full-tree pass, or only fall back to the O(n)
   shortcut if that pass doesn't resolve the node), or add similarity-based anchoring (parameter-
   list shape / identifier Jaccard similarity) as a fallback when name-based anchoring fails.

*  `rust-algorithm-change` (optimal_solutions test): the human-authored ground truth matches
   before's OUTER `for` loop to after's (only) `for` loop, deleting the whole INNER (nested) loop
   and inserting after's `if`/`return`/`seen.insert` body as new. codediff instead matches the
   INNER loop to after's loop and reuses its `if`/`return` body, because that body is a much
   closer syntactic match to after's `if`/`return` body than the outer loop's body is (which wraps
   an entire second `for` loop) - reuse is cheaper than delete+insert under unit cost, so this
   isn't a coin-flip the DP happened to lose.
   Checked this isn't a reachable-but-mistied case: summed the edit cost implied by the complete
   human mapping in `human_mapping.json` (each `delete`/`insert`/`update` entry = 1,
   `insert_with_children` = its subtree size) against codediff's actual root mapping cost from the
   same pipeline - human-implied cost is **96**, codediff's is **10**. That's not a tie needing a
   tiebreak; the human's reading is ~10x more edit operations because it requires recognizing that
   the whole loop got algorithmically replaced (brute-force nested loop -> HashSet single loop) and
   deliberately *not* reusing the syntactically-similar `if`/`return` shape. Pure syntactic
   tree-edit-distance has no signal for that - it only ever minimizes edit operations, so it will
   always prefer reuse when reuse is available and cheaper, regardless of whether the reused code
   is semantically related. Not fixable without changing the objective itself (favor "replace this
   whole container wholesale" over minimizing token-level edit script size in some cases), which is
   a deliberate design tradeoff to weigh - not a bug, and not something a local cost-model tweak or
   DP tie-break can produce. Three ways to actually move on this, for whoever picks it up:
   (a) accept as a known limitation of syntax-only diffing and leave the test un-green or delete it,
   (b) pursue an explicit "prefer replacing a whole matched container over token reuse past some
   depth/size" heuristic as its own effort (same risk class as the reverted hash-based
   pre-matching pass mentioned in `resolve_forest` - arbitrary interior-node bias has bitten this
   codebase before, so it needs its own careful validation against the full optimal_solutions
   suite, not just this one case), or (c) reconsider whether this particular hand-authored ground
   truth is asking for algorithmic/semantic understanding that's out of scope for an AST-structural
   differ.
