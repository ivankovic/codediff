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
