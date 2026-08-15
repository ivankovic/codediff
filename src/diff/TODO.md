# Diff Module Notes

This document originally listed 10 "bugs" from an AI analysis pass on 2026-07-01. As of
2026-07-08, all ten have been re-verified against the current code: three were real and are now
fixed, six were never actual bugs (the "fix" would have been wrong, or the behavior is
intentional), and one is a real but low-value finding blocked on unrelated in-progress work. Kept
here so nobody re-discovers the same false positives.

## Fixed

- **`row_col_to_char_index` (text_range.rs)**: the trailing `if current_row == row && current_col
  <= col { return char_index }` was dead code - both that branch and the fallback after it
  returned the same `char_index` value, so the condition never changed behavior. Removed.
- **Duplicate hash group collapse (solve_identical_trees.rs)**: previously all before-nodes
  sharing a hash mapped onto the same after-node. Fixed (each duplicate now claims a distinct
  after-node); regression-guarded by
  `duplicate_hash_group_matches_each_copy_to_a_distinct_after_node`.
- **`compute_delta` (apted/engine.rs) is now containment-aware (2026-07-10)** - previously the
  biggest remaining perf lever: `Algorithm::ZhangShasha`'s classic keyroot DP is what made large,
  mostly-rewritten pairs slow (e.g. one method pair in `rust-zed-workspace-tasks` cost >1.2s
  alone), and `Algorithm::Apted` was asymptotically far better but fell back to Zhang-Shasha
  whenever a forest had real pruned-descendant constraints (the common case, since
  `pre_match_by_path`/method pre-matching is exactly what makes containment non-trivial) because
  its `compute_delta`/`vren` never applied `ContainmentCtx`. `EngineCtx` gained a
  `containment: Option<&ContainmentCtx>` field; `vren_adjusted` (engine.rs) is the single place
  every real `vren` call site routes through - `spf_a`'s `ren_cost` closure (covers all 4 of its
  invocations), plus one site each in `apted_tree_edit_dist` (spfL) and `apted_tree_edit_dist_r`
  (spfR). `spf1`'s three `vren` calls were deliberately left unadjusted: it writes nothing to
  `delta` and its return value is discarded at every call site (the top-level `gted(0,0)` return is
  dropped, off-path recursion drops returns, and the vroot-expansion branches only sum into a
  discarded total), so containment there would be dead code. `resolve_forest`'s
  `Algorithm::ZhangShasha` fallback (the `if containment.is_trivial() { ... }` gate) is deleted -
  Apted is now used unconditionally when selected - and `ContainmentCtx::is_trivial()` itself was
  deleted as now-dead code.
  Verified in three independent ways: (1) a new fuzz test,
  `test_apted_engine_matches_oracle_fuzz_with_containment` (apted/common.rs), generates forests
  with genuine pruned-descendant constraints (`gen_random_pruning`: random leaf-to-leaf pre-matches
  on same-kind-attractive trees) and asserts Apted-with-containment matches
  Zhang-Shasha-with-containment - confirmed to have teeth by individually disabling each of the 3
  `vren_adjusted` sites and observing a real cost divergence (e.g. "new engine cost 21 != oracle
  cost 20") before restoring; (2) a full `cargo test --lib` run before (code stashed) vs. after
  showed **byte-identical FAILED-test sets** (14/14 match exactly, zero new regressions, one net
  new pass from the new fuzz test itself) and **35% faster wall time** (590s vs. 902s on the dev
  box the change was verified on); (3) `benchmark_optimal_solutions` TOTAL is unchanged, 771
  mismatches / 0.27% / 4 unsolved, identical before and after.

- **`resolve_flat_tree_pair`'s pooled Myers input discarded its own anchors (apted/common.rs,
  2026-08-14)**: the flat-tree fast path (`FLAT_MIN_CHILDREN`-gated Myers sequence diff) built its
  input by filtering a parent's children down to the still-unmatched ones (`flat_children`) before
  ever running Myers - which throws away exactly the already-matched siblings (e.g. XML `element`s
  matched by `nodes::is_reference` earlier in the pipeline) that would otherwise anchor a run of
  hash-identical children (XML whitespace `CharData` between them). With no anchors left in the
  sequence, a run of N indistinguishable entries with one insertion/deletion gives Myers N
  tied-optimal alignments, and its own tie-break (not ground truth) decided which one "moved" -
  drifting every remaining entry in the run by one. Confirmed via a live case
  (`xml-nextcloud-android-delete-element`: `flat_children` returned 1140 unmatched children out of
  2277 total on the `content` node - the other 1137 already-matched `element`s were silently
  excluded from the Myers input entirely). Fixed by splitting the *full* child list into segments
  at already-matched boundaries first (`split_into_anchored_segments`) and running the existing
  `myers_lcs` once per segment instead of once over the whole pooled list; reduces to the old
  single-pool behavior whenever nothing is matched yet. `flat_children` now returns the full list
  (still gated on the unmatched-child count, same threshold as before). Fixed all three affected
  fixtures to 0 mismatches (857/910/591 -> 0/0/0); full corpus benchmark shows zero regressions
  elsewhere and a net -2359 mismatches (23449 -> 21090). Purely a mismatch-count fix on these
  fixtures - `algorithm_cost == human_cost` was already true, so rendered diff output is unchanged;
  the residual tie between "whitespace before" vs. "whitespace after" a single deletion is real and
  unavoidable, this just stops it from propagating past its own local segment.

- **`solve_similar_flow_control` (MatchSimilarFlowControl) deleted (2026-08-14)**: phase 4's
  arm-overlap matcher for still-unmatched `if`/`switch`/`match` constructs. Net-negative in the
  2026-07-15 ablation study (disabling it individually *improved* the benchmark by 82 mismatches),
  disabled by default ever since, and never re-enabled - so removing it changes nothing about
  default-config diff output. Confirmed: full corpus benchmark identical before/after (21090
  mismatches both times, zero fixtures changed). Removed the module
  (`solve_similar_flow_control.rs`),
  its `HeuristicConfig::solver_similar_flow_control` gate and CLI flags
  (`--solver-similar-flow-control`/`--no-solver-similar-flow-control` on
  `benchmark_optimal_solutions`), and its now-solely-owned helpers in `nodes.rs`
  (`FlowControlArm`, `flow_control_arms`/`match_arms`/`switch_arms`/`if_chain_arms`,
  `signature_text`, `trimmed_text`, `flow_control_signature_set`). Kept: `FlowControlFamily`/
  `flow_control_family` (still used by `is_block_container` for `solve_greedy_anchor_blocks`),
  `flow_control_similarity_of_sets` (still used by `solve_import_list_overlap`'s Jaccard scoring),
  `nodes::collect_unmatched` (still used by `solve_identical_diagnostic_statements`). Phase 4 is now
  three mechanisms sharing `grouped_greedy_matcher`'s generic engine (named-group matching, import-
  list overlap, positional anchoring) plus the one that doesn't fit that shape
  (`solve_large_flat_subtrees`), not four.

- **Phase 4 naming cleanup (2026-08-14)**: `solve_named_reference_groups`/`_within` (and its
  helpers `match_named_groups`, `collect_fully_resolved_groups`/`_excluding_root`/`_rec`) renamed
  to `solve_qualified_name_groups`/`_within`/`match_qualified_name_groups`/
  `collect_qualified_name_groups`/`_excluding_root`/`_rec` - "reference" collided with the
  unrelated `nodes::is_reference` (phase 1's own-identity predicate for XML elements, imports,
  etc.), and the old name didn't foreground the actual mechanism (fully-resolved, scope-qualified
  names, e.g. `"Bar::new"` not bare `"new"`). Its `ASTMappingReason::APTED` source strings renamed
  to match: `"syntax_named"` -> `"qualified_name"`, `"syntax_import_list"` ->
  `"import_list_overlap"` (dropping the `syntax_`-prefix-by-parent-module convention in favor of
  direct names, consistent with sibling reasons `"large_flat_subtree"`/`"greedy_anchor_block"`
  which never had that prefix). `apted/common.rs`'s `PREMATCH_SIBLING_ORDER_SOURCES` array (which
  matches against these exact source strings at runtime to gate a real behavior, not just cosmetic)
  updated in lockstep - a pure string-literal rename there would have silently disabled that check
  for this source. Pure rename, no logic changed: full corpus benchmark identical before/after
  (21090 mismatches both times, zero fixtures changed), only the reason-column names in the CSV
  changed (`APTED:syntax_named` -> `APTED:qualified_name`, `APTED:syntax_import_list` ->
  `APTED:import_list_overlap`).

## Not actual bugs (re-verified 2026-07-08)

- **`from_treesitter_range`'s `end_row < columns_per_row.len()` guard**: the original report
  claimed this should be `<=`. That would index `columns_per_row[end_row]` out of bounds exactly
  when `end_row == columns_per_row.len()` - i.e. it would introduce a panic. The `<` guard is
  correct: when `end_row` already equals the array length, the position is already in this
  module's normalized "(next row, 0)" form, so there's nothing to adjust. See the doc comment on
  `from_treesitter_range` for the normalization convention.
- **`TextRange::is_zero`**: not a general emptiness check by design - it detects specifically the
  `(0,0)-(0,0)` sentinel produced by `zero()`, e.g. "no range accumulated yet" in `text.rs`'s
  range-building loop. Doc comment now spells this out.
- **Python method double-diffing (solve_semantically_structural_nodes.rs)**: methods are
  pre-matched individually via `apted::for_nodes`, then the whole class is diffed via another
  `for_nodes` call. This looks like double work but isn't: `PostorderIndexer` prunes any node
  already present in `diff`'s node maps (plus its subtree) before building the forest, so the
  class-level call skips every already-matched method. This is the same intentional
  pre-match-then-diff-container idiom every phase-4 heuristic uses (e.g.
  `solve_greedy_anchor_blocks::anchor_pair_via_apted`).
- **Cost model treats `Move` as free (`COST_MOVE = 0`)**: deliberate, not an oversight.
- **`DeltaTable::get`'s "no bounds checking"**: it indexes a `Vec`, which panics on out-of-bounds
  access like any other Rust indexing - there's no silent corruption to guard against. Returning
  `0` for an *unset-but-in-bounds* cell is intentional (see `UNSET` sentinel).

## Known, deliberately unenforced (not fixed here - architectural, not a readability fix)

- **`NodeCache`'s `'static` lifetime (diff.rs)**: real - the transmute is unenforced by the type
  system, so nothing stops a future caller from stashing a `NodeCache` past the `Code` it borrows
  from. Already documented in detail on the struct itself (see its "Safety invariant" doc
  comment) rather than silently exposed; fixing it for real (`Rc<Tree>`, an `OwnedNode` wrapper,
  or compile-time lifetime plumbing) is a real API change, out of scope for a readability pass.
- **Potential `u64` overflow in `sz * (sz + 3) / 2` (apted/engine.rs)**: technically true, requires
  a subtree with roughly 2^32 nodes to actually overflow - not realistic for a source file AST.
  Left as is.

- **`ASTMetadata::node_to_parent` and `ContainmentCtx`'s hot maps switched to `FxHashMap`
  (2026-07-16)** - investigated "are there algorithmic runtime optimizations available" and found
  a genuine hash-seed-dependent performance bug, not a logic bug: on `kotlin-nextcloud-a-few-
  small-removals`, separate process invocations of the *identical* diff (same binary, same input)
  varied 2.8s-26.4s in wall time (confirmed CPU-bound via `/usr/bin/time -v` - `User time` ≈
  `Elapsed` on every run, ruling out scheduling/IO noise - and confirmed the *output* was
  byte-identical every time by fingerprinting the exact set of nodes matched right before the
  final APTED pass, sorted by stable `start_byte`, across 6 runs spanning both fast and slow
  cases). Root cause: `ContainmentCtx::adjust` (`apted/common.rs`) calls `is_ancestor_or_self`,
  which walks `HashMap<usize, usize>` ancestor-parent maps in a loop, on every `vren_adjusted`
  call inside APTED's core DP - an enormous number of lookups on any fixture with real
  containment. `std::collections::HashMap`'s default hasher (`SipHash`) is correctness-fine but
  randomly reseeded per process, so its collision behavior for this specific integer key set
  varies run to run. Fixed by switching `ASTMetadata::node_to_parent` and `ContainmentCtx`'s
  `before/after_pruned_targets`/`before/after_parents` to `rustc_hash::FxHashMap` (new dependency,
  `rustc-hash = "2.1"`, no transitive deps) - unseeded, so performance is deterministic, and
  faster on small integer keys regardless. Verified: `benchmark_optimal_solutions` output
  byte-identical before/after (782/782 mismatches, 0 differing fixtures across all 86), full
  `cargo test --lib` still 336 passed/0 failed/5 ignored, and the aggregate benchmark got ~10%
  faster (318s -> 285s) with zero change to any result.
  **Follow-up (same day, with `perf` access after `kernel.perf_event_paranoid` was lowered to
  1)**: this fix alone did not solve the extreme variance on `kotlin-nextcloud-a-few-small-
  removals` (still 2.9s-28s across repeated runs after it landed). `perf record --call-graph fp`
  on an actual slow run found the real dominant cost: 42% self-time in `__memset_avx2_unaligned_
  erms`, called via deeply recursive `gted` -> `compute_delta` -> `resolve_forest` ->
  `rayon_core::join::join_context` - i.e. inside `solve_semantically_structural_nodes.rs`'s
  parallel per-pair APTED pre-matching (`class_pairs`/`impl_pairs`/`other_pairs`.
  `par_iter().for_each`), not the final APTED pass. Root cause: all three pair lists are built by
  iterating `semantically_structural_nodes` (a `HashMap<(String, String), usize>`) with no sort,
  then processed in parallel while sharing (and mutating) `diff` under a single `Mutex`. Candidate
  order is therefore hash-seeded, and since each pair's `pre_match_by_path`/`apted::for_nodes`
  call can claim shared descendants before a later pair gets to see them, *processing order
  controls how much APTED work gets thrown away* - confirmed by ruling out the alternative
  (`RAYON_NUM_THREADS=1` still showed the same 2.8s-27s variance, ruling out genuine thread
  contention) before finding this via `perf`.
  **Tried and reverted**: sorting `class_pairs`/`impl_pairs`/`other_pairs` deterministically before
  the parallel pass - by document position first, then by largest-subtree-first (mirroring
  `solve_identical_trees`/`hash_tree_matching`'s own convention, on the theory that letting big
  pairs claim shared territory first would minimize wasted small-pair work). Both eliminated the
  *run-to-run* variance (tight, single-digit-percent band instead of 2.8s-28s) but both landed on
  the *slow* end and made the full 86-fixture aggregate benchmark measurably worse (~400s vs. the
  285s baseline from the `FxHashMap` fix above) - the natural hash-random order apparently lands
  in a "wasted work" case *less* often than either fixed order tried. Correctness was unaffected
  both times (782/782 mismatches, 0 of 86 fixtures differing) - this is a performance-only
  regression, confirming the mechanism but not yet yielding a net-positive fix.
  **Real fix, not yet attempted**: the pairs aren't actually order-independent despite the
  "process ... in parallel since they are independent of each other" comment above them - a
  proper fix would decouple them for real (give each parallel task its own private `ASTDiff`
  scoped to just its own pair, with no shared mutable state to race on, then merge every task's
  results back into the real `diff` in one fixed pass afterward) rather than trying to guess a
  processing order that happens to minimize wasted work. That's a bigger, more invasive change
  than this session had scope for.
  **`rayon`/`Arc<Mutex<_>>` removed from `solve_semantically_structural_nodes.rs`, made fully
  single-threaded (2026-07-17)**: requested explicitly as a simplification, with the variance
  finding above already in hand as a caveat going in. Confirmed exactly as predicted: the
  candidate-order bug is unrelated to threading (candidate lists still come from unsorted
  `HashMap` iteration, just processed on one thread now instead of several), so the same
  `kotlin-nextcloud-a-few-small-removals` variance persisted after this change (2.68s-20.99s
  across repeated runs). Correctness unaffected (782/782 mismatches, 0 of 86 fixtures differing;
  full `cargo test --lib` still 336 passed/0 failed/5 ignored). Cost: the full 86-fixture
  aggregate benchmark got ~20% *slower* (285s -> 344s) - fixtures with multiple impl/class/
  function pairs lost real parallel throughput they were previously getting, with nothing gained
  to offset it since removing threading doesn't touch the actual bug. Kept anyway (explicit
  choice): simpler code (no `Arc`/`Mutex`/`rayon` plumbing, ~130 fewer lines) now, real fix
  (private per-pair diffs, see directly above) can build on top of this simplified version later
  without also having to unwind the parallel-processing machinery at the same time.
  **Unrelated idea surfaced while discussing this (NOT a fix for the above)**: GumTree
  (Falleri et al., ASE 2014) never runs an exact tree-edit-distance algorithm (it uses RTED,
  APTED's predecessor) on anything but a small residual inside an already-matched container - its
  bottom-up phase's Dice-coefficient threshold is a hard gate on *when* exact TED is worth paying
  for at all; anything that doesn't clear it just gets left as a plain delete+insert instead of an
  expensive search. This codebase's `final_pass` does the opposite - full APTED on *whatever's
  left over* after every heuristic pass, uncapped by size. Capping it (fall back to something
  cheap above some residual-size threshold) would bound `final_pass`'s own worst case, which is a
  real, separate, already-documented risk (see "Zhang-Shasha blowup on large residuals" in
  [[diff-perf-pass-2026-07-09]]) - but it is explicitly **not** a fix for the bug documented just
  above: that bug lives in `solve_semantically_structural_nodes.rs`'s *parallel, per-pair* APTED
  calls (each already on a small, single-container subtree, nowhere near `final_pass`'s uncapped
  whole-residual call), so a size cap on `final_pass` wouldn't touch it at all. Worth doing on its
  own merits, not as a substitute for the real fix noted directly above.

- **`solve_flat_macro_bodies` extracted into its own module, `solve_large_flat_subtrees.rs`, and
  generalized from "Rust `macro_invocation`, matched by macro name" to any top-level item in any
  supported language (2026-07-17)**: first step of a larger, explicitly-requested rework of
  `solve_semantically_structural_nodes.rs`. New identity mechanism: `nodes::is_semantically_
  structural`'s existing cross-language (kind, name) extraction, falling back to the original
  macro-callee-name extraction for `macro_invocation` specifically (the one kind `is_semantically_
  structural` doesn't cover - it's about compiler-enforced-unique declarations, a macro invocation
  is neither). For each matched top-level pair, finds the single largest-by-direct-child-count
  descendant on each side (any kind, not just `token_tree`); if both exist, diffs that flat pair
  first (triggers `resolve_forest`'s existing Myers fast path), then the top-level pair itself
  (flat descendant already in `diff` -> pruned). Wired in as its own `solver_large_flat_subtrees`
  pipeline step (default on), running before `solve_semantically_structural_nodes` so an impl/
  class that also contains a large flat blob gets it pre-empted first.
  **Performance regression found and fixed in the same session**: the naive version (BFS every
  candidate's full subtree, unconditionally) made the full 86-fixture benchmark ~4% slower overall
  (344s -> 358s) despite firing on only 1 of 86 fixtures (`kotlin-nextcloud-remove-function`) -
  scanning cost paid by every fixture, benefit realized by almost none. A `node_to_subtree_size`
  pre-filter (skip descending into subtrees too small to possibly qualify) barely helped (358s ->
  355s): most real functions/impls have well over 50 *total* AST nodes even though no single node
  inside has 50 *direct* children, so that filter's early-exit almost never fired. Real fix: added
  `ASTMetadata::node_to_widest_subtree_node` (`code.rs`/`code/metadata.rs`), a new field
  precomputed once per file (bottom-up, alongside `node_to_subtree_size`) giving every node the
  `(count, node_id)` of the node with the most direct children anywhere in its own subtree -
  turns `largest_flat_container_in` from an O(subtree size) BFS into an O(1) lookup. Result: 318s,
  at or below every earlier baseline this session, correctness unchanged throughout (782/782
  mismatches, 0 of 86 fixtures differing at every step; full `cargo test --lib` 338 passed/0
  failed/5 ignored, +2 from the new module's own tests).

## Real, still open

- **`apted::for_nodes`/`for_roots` return `Result<()>` that can never be `Err`** -
  `resolve_forest` isn't fallible, `for_nodes` just wraps its call in `Ok(())` unconditionally.
  Every call site does `let _ = apted::for_nodes(...)`, silencing an error that can't occur. The
  clean fix is to drop the `Result` and have both functions return `()`, then remove `let _ =` at
  every call site - but 9 of the 12 call sites are in `solve_semantically_structural_nodes.rs`,
  which had unrelated in-progress work at the time of this note. Do this once that settles.

## Discovery Information

**Original discovery:** AI analysis using Mistral Vibe, 2026-07-01.
**Re-verified:** 2026-07-08, against `src/diff/` as of commit `4c16008` plus the in-progress
`solve_semantically_structural_nodes.rs` changes.

---

# Algorithm Improvements Backlog

This section tracks novel algorithmic and heuristic improvements to the diff algorithm, beyond implementation
optimizations. Organized by priority phase.

## Architecture rethink: target goals (set 2026-08-15)

The current 7-phase pipeline (hash descent -> leading-sibling/diagnostic matching -> bottom-up
expansion -> syntax-aware matching -> second bottom-up expansion -> final APTED/fast fallback ->
moved-subtree recovery) is judged to have hit its practical ceiling, particularly phases 4/5/6/7.
A full architectural rethink is planned. These are the concrete targets it must hit - not
aspirational, the bar for "done":

- **Latency: p99 < 400ms, p50 < 100ms (ideally).** Corrected baseline, full-corpus
  `benchmark_optimal_solutions --csv` run (332 fixtures, release binary, 2026-08-15): p50 123.9ms,
  p90 1.35s, **p99 61.8s**, max 169.3s. (An earlier note in this section said p99 9.4s - that was
  measured on a narrower scope and is wrong; this is the full-corpus figure. p99 needs a ~150x
  reduction, not 23x.) The tail is bimodal: a few deliberately-gigantic stress fixtures (max case
  is 153129 nodes) where high cost is at least defensible, versus mid-size shape-driven pathology
  where it isn't - e.g. `vimscript-neovim-neovim-add-two-functions-and-modify-a-few-lines` takes
  87s at only 11647 nodes, while `json-iwalton3-jellyfin-web-...` does 258504 nodes in 9.4s. A 22x
  smaller file taking 9x longer is the existence proof that the tail is APTED/flat-residual shape
  pathology in phase 6, not raw scale - that's the real target for a redesign, not the huge-file
  cases. Whether the p99 SLO is meant to cover the deliberately-gigantic fixtures at all is an open
  question.
- **Quality: 90% of corpus datasets with zero mismatches; the remaining 10% capped at <=0.5%
  of nodes mismatched.** Baseline (same run): 246/332 fixtures (74.1%) at exactly zero mismatches -
  52 fixtures short of the 298 (90%) needed. 85 fixtures are nonzero and must shrink to <=33 (10%)
  to fit the allowance, each individually <=0.5%; 34 of the 85 currently exceed that cap. Total
  mismatch instances corpus-wide: 21090, but this figure is dominated by one fixture,
  `css-shadcn-ui-ui-completely-broken-treesitter-parsing` (16277 mismatches, 49.8% of its own
  32682 nodes) - a parse failure, not a matching failure; no pipeline redesign fixes a broken
  parse.
  Whether that fixture (and 2-3 similarly-named `broken`/`awful-string-matching` fixtures) should
  count against the quality target is an open question.
  Cost-model breakdown across the 85 nonzero fixtures (`algorithm_cost` vs `human_cost`): 10 are
  true ties (cost function under-discriminates, see the correction below), 57 have
  `algorithm_cost > human_cost` (search failure - the optimum exists but wasn't found; median gap
  is only 10, so mostly near-misses not catastrophes), and 18 have `algorithm_cost < human_cost`
  (the human's mapping isn't actually cost-minimal under the current model - a second, distinct
  cost-function defect). So ~28 fixtures point at the objective function being wrong and ~57 point
  at the search not reaching the objective's optimum - a redesign needs to fix both, and they are
  different problems.

**Correction to prior framing:** it was previously assumed that cases where
`algorithm_cost == human_cost` but the mapping still differs from the human ground truth represent
an unreachable floor (indistinguishable ties, no cost function could pick the human's answer over
the algorithm's). **This is wrong and should not be used to write off any remaining mismatch
bucket.** Human solutions are defined as optimal ground truth; if the algorithm's mapping ties the
human's mapping on cost but differs from it, that proves the cost function itself is
under-discriminating - it is failing to capture some signal the human is implicitly using to
prefer their mapping over the algorithm's. The fix in that case is a better cost function, not
acceptance of the tie. Every `algorithm_cost == human_cost` mismatch is a cost-function bug report,
not evidence of a floor.

## Phases 4-7 replacement: text-diff-first matching (in progress, started 2026-08-15)

Superseding architectural work - see `~/.claude/plans/iterative-herding-panda.md` for the full
design and phased build plan. Phases 1-3 (hash descent, leading-sibling/diagnostic matching, bottom-
up expansion) stay; phases 4-7 (syntax-aware bundle, second bottom-up expansion, whole-residual
final APTED, moved-subtree recovery) are being replaced by a pipeline built around a
parser-independent textual diff computed first, which bounds what the tree diff can contain per
region and lets each region pick the cheapest sufficient algorithm.

### Phase 0 findings (2026-08-15)

**332 vs. 338/339 fixture-count discrepancy, resolved**: the checked-in
`research/optimal_solutions_benchmark.csv` (332 rows) is simply stale - it predates 7 fixtures added
by an earlier commit this session (`rust-skim-rs-skim-format-string`,
`scala-sirthias-parboiled-value-change`, `shellscript-hgst-libzbc-add-variable`,
`shellscript-maxsatula-ocp-small-change`, `tsx-keybase-client-emoji-to-native`,
`tsx-rektdeckard-departure-mono-import-path`, `vimscript-fedorenchik-qt-support-add-two-lines`), and
includes 2 fixtures (`rust-completely-unrelated-main-files`, `rust-hash-optimization`) that exist as
fixture directories but aren't wired into any `.rs` test via `assert_matches_human_mapping`. The
authoritative fixture population is whatever `test::helper::handmade_test_code_pairs()` finds on
disk (currently 339 fixture directories under `src/test/data/diffs/*/*/`, 338 with
`human_mapping.json`, 1 "unsolved") - this is what `benchmark_optimal_solutions` (no args) actually
iterates, and is the correct denominator for every phase's quality gate going forward, not 332.

**Hunk-level insert/delete-only licensing invariant, validated (not just whole-file)**: built a
temporary diagnostic (`src/bin/hunk_census.rs`, deleted after use - dumps every human-mapping
operation's before/after byte range per fixture) and cross-checked it against a line-level text
diff. Refined the test from the original naive "any delete inside an insert-classified hunk's byte
range" (which produces false positives from ordinary ancestor `MatchButNotIdentical` entries that
legitimately span a real change happening elsewhere in their subtree) to the precise signal: **a
mapping entry whose before-byte-range and after-byte-range contain byte-for-byte identical content,
but whose operation isn't `Identical`** - this is the unambiguous signature of the wrap/reparent
counterexample (content unchanged, but still needed real tree work because its structural context
changed).

Result: 4452 non-`Identical` mapping entries checked corpus-wide, only 19 exhibit this signal,
concentrated in 6 fixtures - **all 6 in the whole-file "mixed" text-diff class, zero in the 72
whole-file `insert-only`/`delete-only`-classified fixtures.** Confirms: (1) the mandatory escape
hatch in the plan is validated as necessary, not paranoia - e.g. `rust-turbopack-module-rule` (9
instances) is exactly the already-known moved-code gap (byte-identical string-literal match arms
relocated to a different position, so `MatchButNotIdentical` rather than `Identical` despite
identical bytes); (2) it's rare (6/338 fixtures, ~1.8%) and entirely confined to `mixed`-classified
files - the 72 whole-file-licensed fixtures Phase 3a targets have zero occurrences, so the
constrained-LCS-only path for those fixtures is not expected to need its escape hatch in practice,
even though the hatch must still exist for correctness.

**Full-corpus fresh baseline run**: initially blocked by severe pre-existing memory pressure on the
dev machine (other running applications, not codediff, had already exhausted free RAM and swap
before any benchmark run started) - two attempts (tracked background, detached `setsid`) were
killed. A third, also detached, eventually completed (~19 minutes wall clock) once memory pressure
eased; installed as `research/optimal_solutions_benchmark.csv` (339 fixtures - see the count
resolution above). Final pinned pre-rearchitecture baseline: **252/339 (74.3%) at exactly zero
mismatches, p50 120.2ms, p90 1347.9ms, p99 63576.8ms, max 192233.3ms, 21109 total mismatch
instances** - consistent with (and superseding) the earlier stale-CSV-derived figures quoted in
"Architecture rethink: target goals" above (74.1%/123.9ms/1.35s/61.8s/169.3s/21090 on the 332-row
stale CSV).

### Phase 1 result (on `phases-4-7-rearchitecture` branch, not main - see commit `bf79bfe`)

Latency win confirmed on the two shape-pathology fixtures that motivated this whole rearchitecture:
`vimscript-neovim-neovim-add-two-functions-and-modify-a-few-lines` 87s -> 0.12s (>700x),
`vimscript-neovim-neovim-add-test-case-plus-edit-existing-one` 61.8s -> 0.96s (64x). The two
genuinely-huge fixtures improved but aren't at target yet:
`rust-real-logic-change-in-a-huge-75k-node-file` 169s -> 24.4s,
`tsx-excalidraw-excalidraw-huge-file-with-real-logic-change` 138s -> 16.3s - remaining cost is now
in phases 1-5 (parsing/hashing/candidate-matching at 75k-150k node scale) or the fallback's own
O(residual) Myers pass, not phase 6 - worth profiling once Phase 2/3 land, not blocking right now.
Quality cost is the measured regression documented in the Phase 1 commit message (175/257 fixtures
that relied on real APTED dropped from 0 to nonzero mismatches) - expected to be won back by
Phases 2-3, which is why this stays off `main` until then.

### Phase 2 result: `solve_bottom_up_propagation` (branch commit follows)

Started winning back the quality Phase 1 gave up, as intended. Full 339-fixture corpus, measured
in isolation (`--solver-bottom-up-propagation` on vs. off) against the Phase 1 baseline: **0
regressions, 69 improvements**, total mismatches 25980 -> 25883, **zero-mismatch fixtures 75 -> 129
(22.1% -> 38.1%)**, latency delta within noise (+1.5% aggregate `elapsed_ms` across 339 fixtures x
4 repeated runs each - and it's O(n) per sweep by construction, so a real regression would mean a
bug, not a legitimate cost). Clean enough result to flip `HeuristicConfig::solver_bottom_up_
propagation`'s default straight to `true` rather than leaving it as an opt-in ablation knob like
`solver_import_nodes`/`solver_bottom_up_expansion`.

Zero-mismatch fixtures are still well below the pre-Phase-1 baseline (252/339, 74.3%) and the
90% target - expected, this is only phase 2 of the rearchitecture; Phase 3 (text-diff-guided
per-region dispatch) is where the bulk of the remaining gap is designed to close.

## Phase 1: Quick Wins (1-2 weeks, production-ready)

### Commutative Sibling Matching
- **Problem:** Order-independent blocks (struct fields, enum variants, import statements) cause spurious
  mismatches when reordered, even though the content is semantically identical.
- **Solution:** For node kinds marked as commutative (e.g., `struct_pattern`, `field_declaration_list`,
  `import_spec_list`), sort children by a canonical key (field name, import path) before hashing.
- **Files:** `hash_tree_matching.rs`, `code/metadata.rs`
- **Impact:** Eliminates false mismatches from reordering; particularly valuable for Go, Rust, Python.
- **Complexity:** Medium
- **Dependencies:** None

### Multi-Level Normalized Hashing
- **Problem:** Refactorings that only change variable names or string literals break hash matching,
  even though structure is preserved.
- **Solution:** Compute hashes at multiple normalization levels:
  - Level 0: Full hash (existing)
  - Level 1: Structural hash ignoring leaf values (existing)
  - Level 2: Structural hash ignoring punctuation/whitespace
  - Level 3: Structural hash with normalized literals (all strings = "", all numbers = 0)
  - Level 4: Structural hash with placeholder identifiers (all ids = "ID")
- **Files:** `code/metadata.rs`, `hash_tree_matching.rs`
- **Impact:** Catches refactorings with only name/literal changes; significant for large codebases.
- **Complexity:** Medium
- **Dependencies:** None

### Adaptive Cost Model
- **Problem:** Current unit cost model (1 for insert/delete/update, 0 for move) doesn't reflect real
  refactoring costs. Identifier renames are cheap; type changes are expensive.
- **Solution:** Make costs context-dependent:
  - Identifier updates: cost 1 (common in refactorings)
  - Literal changes: cost 2 (medium)
  - Type changes: cost 5 (expensive, semantic)
  - Internal nodes: cost 0 (handled by children)
  - Large block moves: cost 0 (very common, intentional)
- **Files:** `diff/apted/common.rs` (UnitCostModel)
- **Impact:** Better APTED results; fewer spurious mismatches on renames.
- **Complexity:** Low
- **Dependencies:** None
- **Status:** IMPLEMENTED (2026-07-14)

### Import Path Normalization and Matching
- **Problem:** Import statements are often reordered or have formatting changes that shouldn't count as
  semantic changes. Import hash matching is blocked by path formatting differences.
- **Solution:** Normalize import paths (remove quotes, normalize separators, handle relative imports)
  and match imports by normalized path rather than syntax. Use Move operation for reordering.
- **Files:** `solve_import_nodes.rs` (new module)
- **Impact:** Better handling of dependency changes; reduces noise in diff output.
- **Complexity:** Low
- **Dependencies:** None
- **Status:** IMPLEMENTED (2026-07-14)

## Phase 2: Medium Effort (2-4 weeks, production-ready)

### Type Signature Matching
- **Problem:** Function renames break all existing heuristics. Functions with identical type signatures
  should be matched even if names differ.
- **Solution:** Extract and normalize type signatures (remove whitespace, normalize aliases). Match
  functions by signature similarity when name-based matching fails. Run after semantic node matching,
  before orphan deletion.
- **Files:** New `solve_type_signature_matching.rs`, `code/metadata.rs`
- **Impact:** Handles function renames; critical for large refactorings.
- **Complexity:** Medium
- **Dependencies:** Type extraction infrastructure (may need language-specific parsers)

### Variable/Identifier Rename Tracking
- **Problem:** Variable renames within a function break bottom-up matching and create noise in diffs.
- **Solution:** Build a rename graph from type information and usage patterns:
  - Group identifiers by type
  - Match identifiers with same type across sides
  - Use positional information for tie-breaking
  - Feed rename graph into APTED as hints
- **Files:** New `solve_identifier_renames.rs` or integrate into `solve_bottom_up_expansion.rs`
- **Impact:** Tracks identifier changes within scopes; cleaner diffs for refactorings.
- **Complexity:** Medium
- **Dependencies:** Type information extraction

### Wrap/Unwrap Pattern Detection
- **Problem:** Wrapping code in a new container (if statement, try-catch, etc.) appears as structural
  changes, but the wrapped content is identical.
- **Solution:** Detect containment relationships:
  - If a node's hash in `before` appears as a subtree in `after` but with a different parent
  - And the parent kind is a container (if, try, loop, etc.)
  - Then it's likely a wrap pattern
  - Remap with Move operation and special reason
- **Files:** New `solve_wrap_patterns.rs` or integrate into `solve_moved_subtrees.rs`
- **Impact:** Handles container additions; reduces false mismatches.
- **Complexity:** Medium
- **Dependencies:** None

### LSH for Approximate Matching
- **Problem:** Comparing every node pair is O(n^2). Need sublinear search for large codebases.
- **Solution:** Use Locality-Sensitive Hashing to find approximate nearest neighbors in hash space.
  Pre-filter candidate pairs for expensive heuristics (APTED).
- **Files:** New `lsh_index.rs` or integrate into `code/metadata.rs`
- **Impact:** Speeds up candidate generation for large codebases; 2-10x speedup on candidate filtering.
- **Complexity:** Medium
- **Dependencies:** `lsh` or `locality-sensitive-hash` crate

## Phase 3: Advanced (1-2 months, research-level)

### Extract Method/Function Detection
- **Problem:** Code extraction (pulling a block into a new function) appears as massive delete+insert.
- **Solution:** Detect containment relationships:
  - Find deleted blocks in `before`
  - Look for those block hashes appearing as subtrees within new functions in `after`
  - Remap with special "Extracted" reason
- **Files:** New `solve_extract_refactoring.rs`
- **Impact:** Correctly identifies extraction refactorings; dramatic improvement for large refactorings.
- **Complexity:** High
- **Dependencies:** Subtree hash matching infrastructure (largely exists)

### Inline Method Detection
- **Problem:** Method inlining (opposite of extraction) also appears as massive changes.
- **Solution:** Detect when a function call is replaced by its body:
  - Find deleted function calls in `before`
  - Find the function definition with matching name
  - Look for the function body hash at the call site in `after`
  - Remap with special "Inlined" reason
- **Files:** New `solve_inline_refactoring.rs`
- **Impact:** Correctly identifies inlining refactorings.
- **Complexity:** High
- **Dependencies:** Function body hash extraction (exists)

### Constraint Satisfaction Matching
- **Problem:** Greedy matching can make locally optimal but globally suboptimal decisions.
- **Solution:** Formulate matching as a Constraint Satisfaction Problem:
  - Variables: nodes in `before` and `after`
  - Domains: possible matches for each node
  - Constraints: type compatibility, structural compatibility, size compatibility, position compatibility
  - Use CSP solver (backtracking with forward checking) to find globally optimal matching
- **Files:** New `csp_matching.rs`
- **Impact:** Better global optimization; reduces cascading mismatches.
- **Complexity:** High
- **Dependencies:** CSP solver library or custom implementation

### Voting System for Heuristics
- **Problem:** Different heuristics work well for different code patterns. Greedy application can
  miss opportunities.
- **Solution:** Implement a voting system where multiple heuristics vote on matches:
  - Each heuristic produces candidate matches with confidence scores
  - Group votes by node pairs
  - Accept matches with sufficient total confidence (weighted by heuristic reliability)
  - Use APTED as tiebreaker for close calls
- **Files:** New `heuristic_voting.rs` or modify `diff.rs` pipeline
- **Impact:** More robust matching; combines strengths of multiple heuristics.
- **Complexity:** Medium
- **Dependencies:** Confidence calibration for each heuristic

## Phase 4: Research (2+ months, experimental)

### Graph-Based Matching (Beyond Trees)
- **Problem:** Tree edit distance assumes hierarchical relationships. Some refactorings (e.g., moving
  code across function boundaries) are better modeled as graph operations.
- **Solution:** Convert AST to a graph where edges represent:
  - Parent-child relationships (tree edges)
  - Variable usage (data flow edges)
  - Function calls (control flow edges)
  - Type relationships (type edges)
  Then apply Graph Edit Distance or Maximum Common Subgraph algorithms.
- **Files:** New `graph_matching.rs`, `ast_graph.rs`
- **Impact:** Handles cross-function refactorings; more accurate for complex changes.
- **Complexity:** Very High
- **Dependencies:** Graph library (`petgraph`), GED/MCS implementation
- **Note:** MCS is NP-hard but practical for code-sized graphs (~1000 nodes)

### Machine Learning-Based Similarity
- **Problem:** Hand-crafted heuristics can't capture all patterns.
- **Solution:** Train a similarity model on pairs of code snippets:
  - Features: AST structure, token sequences, type information, depth, node kinds
  - Label: human-annotated similarity scores (or derived from existing optimal solutions)
  - Model: Random forest, gradient boosted trees, or small neural network
  - Use model to sort candidate pairs before APTED
- **Files:** New `ml_similarity.rs`, training script
- **Impact:** Captures patterns beyond hand-crafted heuristics; continuously improvable.
- **Complexity:** Very High
- **Dependencies:** ML library (`linfa`), training data
- **Note:** Requires offline training on labeled data

### Kernel-Based Tree Similarity
- **Problem:** APTED is O(n^3) in worst case. Need faster approximations for very large trees.
- **Solution:** Use tree kernel methods (e.g., convolution tree kernel) to compute similarity scores
  without full tree edit distance. Use as a pre-filter for APTED.
- **Files:** New `tree_kernel.rs`
- **Impact:** 10-100x speedup on candidate filtering for large trees.
- **Complexity:** Medium
- **Dependencies:** Linear algebra library (`ndarray`)

### Iterative Refinement Pipeline
- **Problem:** Early heuristic matches can prevent better matches later (greedy suboptimality).
- **Solution:** Run the pipeline multiple times with different parameters, keeping the best result:
  - Try different threshold values (Dice coefficient, cost ratios)
  - Try different heuristic orders
  - Score each result against a quality metric
  - Return the highest-scoring diff
- **Files:** Modify `diff.rs`
- **Impact:** Better quality; finds globally optimal matches.
- **Complexity:** High
- **Dependencies:** Quality scoring metric
- **Note:** Increases runtime significantly but can improve quality

## Priority Reference

| Phase | Item | Impact | Complexity | ROI |
|-------|------|--------|------------|-----|
| 1 | Commutative sibling matching | ⭐⭐⭐⭐ | Medium | High |
| 1 | Multi-level normalized hashing | ⭐⭐⭐⭐ | Medium | High |
| 1 | Adaptive cost model | ⭐⭐⭐ | Low | High |
| 1 | Import path normalization | ⭐⭐⭐ | Low | High | **DONE** |
| 2 | Type signature matching | ⭐⭐⭐⭐ | Medium | High |
| 2 | Variable rename tracking | ⭐⭐⭐ | Medium | High |
| 2 | Wrap/unwrap detection | ⭐⭐⭐ | Medium | High |
| 2 | LSH for approximate matching | ⭐⭐⭐ | Medium | High |
| 3 | Extract/inline detection | ⭐⭐⭐⭐ | High | Medium |
| 3 | Constraint satisfaction | ⭐⭐⭐⭐ | High | Medium |
| 3 | Voting system | ⭐⭐⭐ | Medium | Medium |
| 4 | Graph-based matching | ⭐⭐⭐⭐ | Very High | Low |
| 4 | ML-based similarity | ⭐⭐⭐⭐ | Very High | Low |
| 4 | Kernel-based similarity | ⭐⭐⭐ | Medium | Medium |
| 4 | Iterative refinement | ⭐⭐⭐ | High | Low |

---

*Last updated: 2026-07-13*
