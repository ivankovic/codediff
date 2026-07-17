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
  pre-match-then-diff-container idiom `solve_similar_flow_control::anchor_matching_arms` uses and
  documents.
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
