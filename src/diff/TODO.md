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

- **`compute_delta` (apted/engine.rs) isn't containment-aware - this is the big remaining perf
  lever.** A 2026-07-09 perf pass found that `Algorithm::ZhangShasha`'s classic keyroot DP
  (`compute_delta_zhang_shasha`, O(n1·n2·min(depth,leaves)₁·min(depth,leaves)₂)) is what makes
  large, mostly-rewritten pairs slow - e.g. one single method pair in `rust-zed-workspace-tasks`
  cost >1.2s alone. `Algorithm::Apted` is asymptotically far better and already fuzz-verified
  correct (`test_apted_engine_matches_oracle_fuzz`, a 20k-seed shrinker) *for the
  containment-free case*, but its `compute_delta`/`vren` never applies `ContainmentCtx` the way
  `compute_delta_zhang_shasha`'s `forest_dist` calls do - see the comment above the
  `ContainmentCtx::is_trivial()` gate in `resolve_forest` (apted/common.rs). That gate now falls
  back to Zhang-Shasha whenever a forest has real pruned-descendant constraints, which is safe
  (confirmed: identical `cargo test --lib` pass/fail set and identical
  `benchmark_optimal_solutions` TOTAL before/after adding the gate) but means Apted is rarely
  used in practice, since `pre_match_by_path`/method pre-matching (the common case in
  `solve_semantically_structural_nodes`) is exactly what makes containment non-trivial. Measured
  with the gate always tripping on the hot fixtures above, Apted's speedup fully evaporates back
  to the Zhang-Shasha baseline.
  Real fix: thread `Option<&ContainmentCtx>` (or just the two parent maps + pruned-target maps)
  into `EngineCtx` and apply the same `adjust()` logic at each of `vren`'s ~6 call sites in
  `spf_a`/`spf_l`/`spf_r`/`gted`. This changes the DP's hot inner loop in an algorithm that took
  three real, ground-truthed bugfixes (see the comment above `compute_delta`) to get correct in
  the containment-free case - do not ship without a dedicated containment-aware fuzz check (extend
  the existing oracle fuzz to build forests with pruned descendants, or compare Apted-with-
  containment against Zhang-Shasha-with-containment on such forests) alongside the existing
  20k-seed shrinker. Once verified, the `Algorithm::ZhangShasha` fallback in the `is_trivial()`
  gate (apted/common.rs, in `resolve_forest`) can be deleted and Apted used unconditionally.
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
