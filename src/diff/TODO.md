# Diff Module Notes

## 2026-08-20: ranked plan against the visible-node goals

Every pass-level conclusion below this section was drawn on the **all-nodes** mismatch count. The
goals are now stated in visible nodes (README's "Accurate" principle), so the passes were
re-ablated and the failing fixtures re-attributed against the new objective. Several prior verdicts
do not survive. Measurements are from `research/data/quality/optimal_solutions_benchmark.csv` as of
commit `87f0435` - i.e. with visibility structural (item 0), the second tier at 1%, and the
similarity alignment of item 1 shipped - over 468 solved fixtures. Earlier revisions of this
section were measured before those and their numbers should not be quoted.

**Standing** (re-measured 2026-08-20, after `87f0435`). Goal 1 (90% zero visible): 357/468 = 76.3%,
**gap 65 fixtures**. Goal 2 (99% within 1% visible): 448/468 = 95.7%, **gap 16 fixtures**. Runtime
p50 4.8ms (goal <100ms, **met**), p99 1029ms (goal <400ms, **missed**), max 2234ms; 16 of 469
fixtures exceed 400ms and account for 53% of total corpus runtime.

**Where the 2037 visible mismatches across the 111 failing fixtures come from**, by the
`ASTMappingReason` of the mapping codediff actually produced. Counted three ways, because they
rank differently and the goals are per-fixture: total volume, share of the 36 fixtures within 3
mismatches of zero (goal 1's cheapest route to 65), and share of the 20 fixtures above 1% (goal 2's
entire target list is 16).

| producer | fixtures dominated | mismatches | of 36 cheap wins | of 20 above 1% |
| --- | --- | --- | --- | --- |
| `APTED("qualified_name")` | 41 | 546 | 12 | 5 |
| `APTED("fast_fallback")` | 37 | 419 | **17** | **9** |
| no mapping at all | 8 | 365 | 1 | 3 |
| `IdenticalHashOfAncestor` | 4 | 347 | 1 | 1 |
| `APTED("large_flat_subtree")` | 7 | 187 | 4 | 0 |
| `StructurallyIdenticalAncestor` | 1 | 100 | 0 | 0 |
| `MovedSubtree` | 1 | 41 | 0 | 0 |
| `large_flat_subtree_container` / `greedy_anchor_block` | 2 | 30 | 0 | 0 |
| mixed, no single dominant pass | 10 | - | 1 | 2 |

`qualified_name` + `fast_fallback` dominate **78 fixtures**, more than the entire 65-fixture gap to
goal 1. Cost-model framing across the same 111: **88 are search failures** (`algorithm_cost >
human_cost` - the optimum exists and was not found), 13 have `algorithm_cost < human_cost`, 10 are
ties. Overwhelmingly a search problem, not an objective-function problem.

`IdenticalHashOfAncestor` is an artifact rather than a target: it maps every descendant of an
already-matched pair in lockstep, so when the pair is wrong the whole subtree's leaves are wrong
with it. High volume (347) over only 4 dominated fixtures - the root cause is whatever mis-paired
the ancestor.

### 0. RESOLVED 2026-08-20: visibility is now structural, not renderer-derived

The original form of this item reported that goal 2's *denominator* was gameable. On investigation
the numerator was gameable too, and worse. `visible_node_ids` derived visibility from the renderer,
so both ends of the rate moved with the algorithm: a diff that renders coarsely has almost nothing
visible and therefore almost nothing it can get visibly wrong.
`css-shadcn-ui-ui-completely-broken-treesitter-parsing` collapsed 32,682 nodes into **2 rendered
spans** and scored **0 visible mismatches while holding 124 real ones** - a clean pass on goal 1.
(An earlier note in this file cited that fixture as evidence *for* the visible metric. That was
backwards: the metric did not judge those mismatches harmless, our own bad diff hid them.)

Replaced by `nodes::is_structurally_visible`: a node is visible if it carries text of its own -
a leaf, or an interior node with non-whitespace content its children don't cover. A pure function
of the tree and the source bytes, so no diff can move it;
`structural_visibility_does_not_depend_on_what_the_file_is_diffed_against` pins exactly that.
css-shadcn now reads 124 visible of 32,640, as it should.

**Consequence: the 4% threshold needs re-deriving, and every number in items 1-5 below was
measured under the old definition.** Visible nodes went from 3.4% of all nodes to 68.2%, so the
same percentage is a ~20x looser bar. Goal 1 barely moved (352 -> 354 of 468, the extra visible
mismatches concentrated in fixtures that were already failing). Goal 2 went 428 -> **460/468 =
98.3%**, gap 4. What the bar buys at each setting, and what it allows on a median fixture
(1,875 visible nodes, up from 132):

| bar | fixtures passing | gap to 99% | allowance on a median fixture |
| --- | --- | --- | --- |
| 0.5% | 430/468 = 91.9% | 34 | ~9 mismatches |
| **1% (chosen)** | **448/468 = 95.7%** | **16** | **~19** |
| 2% | 455/468 = 97.2% | 9 | ~37 |
| 4% | 460/468 = 98.3% | 4 | ~75 |

4% was chosen to fix an inversion that only existed because the old denominator was tiny; with a
structural denominator that problem is gone and 0.5% is once again a genuine relaxed tier rather
than a synonym for zero. Among the 114 fixtures with any visible mismatch the rate distribution is
median 0.25%, p75 0.75%, p90 2.48%, max 19.35%, so 4% only catches the extreme tail.

### 1. `fast_fallback` - still the top lever, but NOT via a size gate

Leads both target lists: **17 of the 36 fixtures within 3 mismatches of zero** and **9 of the 20
above 1%**. No other producer leads both.

**Correction to this item's earlier form, which named the wrong mechanism.** It previously
recommended size-gating the terminal fallback via Phase 3b's `APTED_REGION_BUDGET`, on the theory
that the fallback substitutes cheap Myers alignment for real APTED to protect p99. Reading the code
disproves that: `resolve_residual_forest_via_myers_lcs`'s equal-count branch **already calls real
APTED, explicitly uncapped** (`common.rs`, "Uncapped in size, same as `resolve_flat_tree_pair`'s own
equal-count branch"). A size budget could only ever *reduce* matching here. The lossy path is not a
size gate at all - it is the **unequal-count** fallthrough, where a gap whose two sides hold
different numbers of substantial unmatched roots is resolved as atomic delete plus insert.

That diagnosis is confirmed by what its mistakes look like: of 419 visible mismatches, **331 are
"the human matched this node and we mapped it to 0"**, 68 are a wrong partner, 20 a wrong
delete/insert. It under-matches; it does not mis-match. Attack "give up less", not "guess better".

**Shipped 2026-08-20 (`87f0435`), partial:** when the unequal-count gap's exact-hash alignment
(`resolve_unequal_segment_via_kind_only_anchors`, `myers_lcs` over `node_to_kind_only_hash`) finds
nothing, fall back to an order-preserving alignment scored by leaf-content similarity
(`node_to_similarity_sketch`) instead. Instrumenting the exact-hash pass first showed why it was
needed: across all 468 fixtures it evaluates **128 candidate pairs, 7 distinct**, only one of which
clears `KIND_ONLY_ANCHOR_MIN_SIZE` - kind-only is coarser than the full hash but still an
*equality* test, so one added statement anywhere inside a subtree prevents alignment. Result: +3
zero-visible fixtures, 1 regression, sum visible 2056 -> 2037.

**Still open on this path**, in the order worth trying:
- The trivial-leaf filter (`TRIVIAL_ENTRY_MAX_SIZE`) drops punctuation entries to unconditional
  delete/insert with the justification that they would be "resolved independently... exactly as an
  unmatched leaf would be resolved on its own anyway". That is the flaw: they could often be
  matched. `cpp-add-templates`' single remaining visible mismatch is exactly this - a `;`
  reparented under a new `template_declaration`, mapped to 0. Pair leftover trivial entries among
  themselves by (kind, position) after the substantial ones are paired.
- `TRIVIAL_ENTRY_MAX_SIZE` was tuned against the **all-nodes** objective. Trivial leaves are
  exactly the nodes that are always structurally visible now, so it was optimised against the wrong
  target. Cheap to re-sweep, and it interacts with the item above.
- Widen the *anchor* key in step 1 (`node_to_full_hash`, byte-identical only), so fewer entries
  land in unequal gaps at all. Prior art: a `KindOnlyHash` sub-anchor attempt needed a size-50 floor
  to be safe, and the similarity sketch is now available as a better trust signal than size.
- 48 of the give-ups are `ERROR` nodes from broken-parse fixtures - a different problem, not worth
  attacking through this path.

`mod.rs`'s `for_roots_fallback` doc comment still describes the deleted `looks_expensive()` gate and
should be corrected whatever else happens here.

### 2. `qualified_name` - most fixtures and most mismatches, but harder

41 fixtures, 546 visible mismatches - the largest single producer by both counts. Investigated
2026-08-17 and found to be *two* distinct problems, commutative reorder and a search-quality gap,
which is why it survived that pass unfixed. It trails `fast_fallback` on both target lists (12 of
the 36 cheap wins, 5 of the 20 above 1%), so it is the bigger prize and the worse starting point.

Import-list and reorder shapes dominate its failures. Worth noting that two of the three fixtures
`87f0435` fixed were import-list cases (`typescript-n8n-io-n8n-remove-and-add-imports`, and the
import churn in `typescript-th-ch-youtube-music-...`), which suggests part of this bucket is
reachable from the residual path rather than needing a dedicated import matcher.

### 3. Three passes are inert under the new objective

Re-ablated under structural visibility (baseline before `87f0435`: 354 zero / 448 within-1%;
"delta" is what *disabling* the pass costs, so positive means the pass helps):

| pass disabled | fixtures w/ visible delta | sum visible delta | sum total delta |
| --- | --- | --- | --- |
| `solver_moved_subtrees` | 29 | **+1650** | +2396 |
| `solver_bottom_up_propagation` | 0 | **0** | +318 |
| `solver_mutual_ancestors` | 0 | **0** | +23 |
| `solver_unique_type_matching` | 0 | **0** | **0** |

- `solve_unique_type_matching` changes **nothing at all** - not one node, visible or invisible, on
  any of 468 fixtures. Added 2026-08-17 from the GumTree Simple literature survey with
  `default = true` "pending full-corpus measurement". That measurement is in: it is dead code.
- `solve_mutual_ancestors` moves 23 invisible nodes and zero visible ones.
- `solve_bottom_up_propagation` fixes 318 **invisible** nodes and zero visible ones. Do **not**
  delete it on that basis: its stated job (`diff.rs:445`) is to shrink the residual *before* the
  terminal fallback sees it, so its value is conditional on item 1 and it must be re-measured after
  further work there. It is exactly neutral on goal 2; under the old renderer-derived metric
  disabling it appeared to *gain* a fixture, which was purely the denominator artifact item 0
  removed.

Only `solve_moved_subtrees` clearly earns its place - disabling it costs 6 zero-visible fixtures.
These ablations also re-confirm item 0's fix empirically: across all four runs, **zero fixtures had
their visible-node denominator change**.

### 4. Runtime: the p99 tail is shape pathology, not scale

The 16 fixtures over 400ms are bimodal and only one half is a real problem. Defensible:
`json-ipfs-ipfs-desktop-...` does **396,812 nodes in 723ms**. Not defensible, same run:
`kotlin-nextcloud-a-few-small-removals` takes **1043ms at 6,892 nodes**,
`swift-swiftlang-swift-actual-logic-change` 1029ms at 6,688,
`typescript-excalidraw-excalidraw-add-values-to-lists` 812ms at 6,170. A 57x smaller file taking
44% longer is the existence proof. p99 needs ~2.6x, and these five mid-size cases are where it is -
a bounded investigation of specific fixtures, not a general performance push. (The corpus is the
fixture set, not the 7,400-repository dataset the README's 99.99% target names; it is the available
proxy, not the goal's own denominator.)

### 5. 8 fixtures whose visible mismatches have no mapping at all

Neither matched nor correctly deleted/inserted - the node simply has no entry. 365 mismatches over
8 fixtures, and 3 of the 20 fixtures above 1%, so it is concentrated rather than diffuse. A distinct
failure shape from items 1-2 and not yet diagnosed; worth one pass to see whether it is one bug or
eight.

---

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

- **Latency: p99 < 400ms, p50 < 100ms (ideally).** **STALE (2026-08-17): every latency figure in
  this bullet is ~5-6x inflated by a benchmark-harness artifact (metadata recomputed ~26x per timed
  diff) - see "Latency baseline was ~5-6x harness artifact" below for the honest baseline: p50
  6.8ms, p99 1.13s, max 4.9s.** Original text kept for history: corrected baseline, full-corpus
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
- **Quality (RESTATED 2026-08-20, now in VISIBLE nodes): 90% of test cases with zero mismatched
  visible nodes; 99% with at most 1% of visible nodes mismatched.** Superseded the previous
  all-nodes phrasing ("90% zero mismatches, remaining 10% capped at <=0.5% of nodes") once
  the visible/invisible distinction became measurable: most AST nodes are structure the
  reader never sees on its own, so a wrongly-matched `block` is not the same defect as a
  wrongly-matched identifier. Both bars are on `visible_mismatches` / `visible_nodes` in
  `research/data/quality/optimal_solutions_benchmark.csv`; `benchmark_optimal_solutions` prints
  progress against both after its tables.

  **Current standing, the threshold history, and the ranked plan all live in this file's
  2026-08-20 section at the top - do not duplicate them here.** Note in particular that visibility
  is now `nodes::is_structurally_visible`, a property of the tree and the source bytes; the
  renderer-derived `diff::text::visible_node_ids` this paragraph originally named is deleted, and
  any figure computed against it (median 132 visible nodes, "3.4% of all nodes", the 4%-vs-0.5%
  reasoning) belongs to that definition and does not transfer.

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
`research/data/quality/optimal_solutions_benchmark.csv` (332 rows) is simply stale - it predates 7 fixtures added
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
eased; installed as `research/data/quality/optimal_solutions_benchmark.csv` (339 fixtures - see the count
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

### Phase 3a result: line-diff core extraction + whole-file classification (branch commit follows)

Pure plumbing, zero behavior change, deliberately scoped that way per the advisor's review before
starting: extracted `plain_text_line_diff`'s hunk-producing core into `line_diff_core`
(`src/diff/text.rs`), added `WholeFileClass` (`Identical`/`InsertOnly`/`DeleteOnly`/`Mixed`) and
`whole_file_text_class`, the entry point Phase 3b's dispatcher will license a constrained-LCS
resolver from. `plain_text_line_diff` itself now calls through the extracted core - refactor, not
reimplementation.

Before building the resolver on top of this, checked whether it's even worth building yet: joined
Phase 0's 72 whole-file-licensed fixtures against the Phase 2 baseline CSV. **22/72 are already at
zero mismatches**; the other 50 have real residual (`c-microsoft-terminal-add-function` 201,
`typescript-excalidraw-excalidraw-add-function` 115, down to several at 1-10) - confirms the
premise, Phase 3b/resolver work is not chasing an empty set.

Validated the new primitive against an *independently computed* ground truth, not just its own
logic: `src/test/data/whole_file_text_classification_census.csv`, 338 fixtures classified via
Python `difflib.SequenceMatcher(autojunk=False)` over `str.splitlines()` (no `keepends` - matching
Rust's `.lines()` semantics, which normalize away trailing-newline-presence differences; the first
census attempt used `keepends=True` and flagged one false mismatch, `cpp-whitespace-only-change`,
whose *only* byte difference is a trailing `\n` at EOF - a real methodology bug in the census
script, not a Rust bug, fixed by regenerating without `keepends`). Cross-check test:
`diff::text::tests::whole_file_text_class_matches_independent_census` (`#[ignore = "slow"]`, full
corpus) - **0 mismatches across 338 fixtures**. Five additional focused unit tests cover
`Identical`/`InsertOnly`/`DeleteOnly`/`Mixed`/give-up-treated-as-`Mixed` directly, per the plan's
own note that this primitive's risk profile changes once it's load-bearing for matching, not just
visualization.

Advisor guidance followed: split what the plan labeled "Phase 3a" into (1) this commit - extraction
+ classification, threaded through with no behavior change, validated against ground truth - and
(2) a separate follow-up commit for the constrained-LCS resolver itself and its wiring into
`PendingDiff::finish`, so a future regression can be attributed to the resolver, not conflated with
the extraction (the same bundling mistake that cost an isolation re-run in Phase 1). Also flagged:
propagation (Phase 2) currently runs only once, before the terminal fallback - a leaf-level match
the Phase 3b resolver creates *inside or after* the terminal step won't bubble up to its ancestors
without a second propagation call after it. Not yet added; tracked for the resolver commit.

### Bug fix: `maximal_unmatched_roots` collapsed the whole file to one atomic block (2026-08-15)

Before starting the Phase 3a resolver itself, checked one small residual fixture
(`rust-add-value-to-enum`, 1 mismatch) with a throwaway debug test to see what it actually needed -
per the advisor's standing guidance to verify a concrete failure before designing around it. Found
something bigger than a missing resolver: `source_file` itself was being marked `Delete` via
`fast_fallback`, even though 89/90-ish of its descendants were already correctly matched.

Root cause, in `apted::common::maximal_unmatched_roots` (the walk `resolve_residual_forest_via_
myers_lcs`/`for_roots_fallback` use to find "maximal still-unmatched subtrees" to align): it
**stopped descending the instant it found an unmatched node - including the root itself.** Any real
edit changes the root's own content hash, so the root is unmatched for nearly every fixture; the
walk then treated the *entire file* as one atomic block and never looked inside it for the smaller,
genuinely-recoverable pockets nested there (e.g. two `attribute_item`s and a byte-identical enum
variant in the `rust-add-value-to-enum` case - none of them "reference nodes" or "big enough" for
`solve_hash_descent`'s own selector, so nothing upstream of the fallback ever touches them either).
This bug predates this session, but was invisible until Phase 1 promoted this function's caller
from a rare `DiffMode::Fast` substitute (gated behind `EXPENSIVE_RESIDUAL_THRESHOLD`, 5000+
unmatched nodes) to the unconditional terminal step - now it fires on every fixture whose root
doesn't hash-match, which is nearly all of them. This is almost certainly the dominant cause of
Phase 1's 175-fixture quality collapse, not an inherent cost of removing whole-residual APTED.

Fix: `maximal_unmatched_roots` now computes (one extra `O(n)` postorder pass, `subtree_has_any_
match`) whether a node's subtree contains *any* matched descendant before deciding to stop there -
only a subtree with *zero* matched nodes anywhere in it is emitted as one atomic block (preserving
the original intent: a genuinely-deleted function still comes out as a single sequence entry, not
one per statement). Verified `add_prune_mappings` (the delete/insert writer both branches of the
fallback use) skips anything already present with a real mapping before assuming this was safe to
change without also auditing every call site - it does, so the fix is confined to *finding* more of
the genuinely-unmatched pockets, not at risk of clobbering an already-correct match.

Also added a second `solve_bottom_up_propagation` call, right after `for_roots_fallback`, gated the
same as the first: newly-fallback-matched small pockets (the attribute_items, the enum variant)
need a chance to bubble up to their now-fully-resolved ancestors, which the first (pre-fallback)
propagation call can't do since those matches didn't exist yet when it ran. Cheap - O(n), no-op
whenever the fallback found nothing new.

Also fixed a stale default while in the area: `benchmark_optimal_solutions`'s `--solver-bottom-up-
propagation` flag defaulted to `false` (`default_value_t = false`), silently diverging from
`HeuristicConfig::default()`'s `true` - meaning every *default* invocation of the benchmark tool
(no explicit flags) was running the corpus *without* Phase 2's propagation. Flipped to match; this
means any earlier single-fixture `--details`/`--dump` diagnostic run without an explicit
`--solver-bottom-up-propagation` flag understated what production code actually does.

**Full 339-fixture corpus result, isolated to just this fix (Phase 2's propagation already on both
sides)**: **zero-mismatch fixtures 129 -> 217 (38.1% -> 64.0%)**, total mismatches 25883 -> 5052 (a
~5x reduction), only **2 regressions** (`html-mozilla-firefox-firefox-remove-li-around-button` 20 ->
24, `kotlin-nextcloud-android-move-from-one-mocking-library-to-other` 48 -> 49 - both duplicate-
content Myers-ordering-ambiguity cases, a known hard category, not a new failure mode), 183
improvements. Latency unaffected (p50 105.7 -> 119.7ms, p99 9036 -> 9741ms, both within noise) -
expected, the fix is `O(n)` by construction like the walk it replaces. Of the 72 whole-file
`InsertOnly`/`DeleteOnly`-licensed fixtures Phase 3a's resolver was about to target: **65/72 (90.3%)
are now already at zero mismatches**, up from 22/72 - this single bug fix did more for that target
set than the planned resolver would have, and substantially shrinks what Phase 3b/3c still need to
do. `research/data/quality/optimal_solutions_benchmark.csv` refreshed to this result.

Zero-mismatch fixtures (64.0%) are now close to the pre-Phase-1 baseline (74.3%) and meaningfully
closer to the 90% target - most of the remaining gap is concentrated in fixtures needing real
per-region dispatch (Phase 3b/3c), not this fallback-traversal class of bug.

Verified with a worktree-based before/after full test-suite comparison (this fix's parent commit vs
after it): 187 -> 58 failing tests, **129 fixed, zero newly broken** - resolves the apparent
discrepancy between "760/58 both times" seen in-session (both runs were already post-fix; no
pre-fix baseline had actually been captured via the test suite until this comparison). The two
previously-`#[ignore]`d Phase 1 regression tests (`moved_function_is_matched_not_deleted`,
`python_leetcode_1_added_if_block_all_ranges`) pass again - un-ignored in a follow-up commit.

### Re-scoping Phase 3a's second half / Phase 3b-3c against the post-fix corpus (2026-08-15)

Per advisor guidance: before building Phase 3a's planned constrained-LCS resolver, re-checked
whether it's still needed. **Don't build it as originally scoped**: of the 72 whole-file-licensed
fixtures, only 7 have residual left (`css-add-property` 4, `go-kubernetes-...-add-unit-test-cases`
5, `shellscript-ansible-...-simple-deletion` 2, `typescript-...-add-target-comment-2` 2, `vimscript-
chikamichi-mediawiki-add-one-autocmd` 31, `vimscript-fedorenchik-qt-support-add-two-lines` 19,
`vimscript-neovim-...-add-a-few-lines-one-after-the-other` 2) - spot-checked `css-add-property`:
an empty `{ }` `rule_set` left unmatched, the same duplicate/near-identical-small-content Myers-
ordering-ambiguity shape as this fix's own 2 regressions, not a delete-forbidden-license gap the
planned resolver would have addressed anyway.

`css-shadcn-ui-ui-completely-broken-treesitter-parsing` (Phase 3c's pinned kill-criterion fixture,
77% of the corpus's original mismatch total): **16277 -> 124 mismatches**, confirming the
architecture-level fix (parser-independent handling, now indirectly via the traversal fix rather
than Phase 3c's planned ERROR-density gate) already does most of what Phase 3c targeted. Phase 3c's
kill criterion needs restating against 124, not 16277, once that phase is actually scoped.

Corpus-wide: 121 fixtures (down from ~210) still have nonzero mismatches. Ran every one through
`--details` and auto-bucketed by the "reason" tag on the wrong mapping (`fast_fallback` on 62
fixtures/2087 mismatches, `qualified_name` on 40/2572 - mostly one outlier -, ~10 fixtures/306
mismatches where the mismatch-checker reports `None` rather than a real decision, and only 4/15
with an explicit before/after *kind* mismatch). Initially mischaracterized the `fast_fallback`
bucket as "duplicate-content Myers-ordering-ambiguity", generalizing too far from this fix's own 2
regressions (which genuinely are that shape). Spot-checking the two largest fixtures in that
bucket - `javascript-microsoft-typescript-broken-js-remove-string-fragment` (602 mismatches) and
`html-mozilla-firefox-firefox-remove-li-around-button` (24) - found a different, more specific, and
more actionable pattern: **long chains of the same repeated node kind** (a left-associative
`binary_expression` chain from string concatenation; deeply nested `element` chains from nested
`<li>`s). Removing one item from such a chain changes every ancestor's subtree hash, so `myers_lcs`
over whole-subtree hashes sees N differing entries where the truth is one deletion plus N-1 relabels
through the nesting - textbook tree-edit-distance territory, not duplicate-content confusion (an
empty `{}` block, like `css-add-property`'s residual, has no nesting to shift and is a different,
still-unexplained case - don't lump them together).

**Confirmed directly**, not just inferred: isolated `apted::for_nodes(Algorithm::Apted, ...)` on
just the smallest enclosing pair for `javascript-microsoft-typescript-broken-js-remove-string-
fragment`'s affected chain (`expression_statement:28`, 206/200 nodes) - bypassing the fallback
entirely - produces **187 Identical, 13 MatchButNotIdentical, 6 Delete**, i.e. essentially the
correct human-like alignment, not a wholesale delete+insert. **Real bounded APTED already solves
this class correctly when given the right region.** Phase 3b's actual remaining job is *finding*
that region and routing to APTED - the resolver doesn't need to be built, only dispatched to. This
is a materially smaller phase than the original plan's "1-2 weeks, largest phase" estimate, which
was sized for a world where APTED itself needed building/bounding, not just invoking.

`kotlin-remove-function` (68 mismatches) is a separate failure *class*: not missing matches but
**over-eager** matching (`solve_qualified_name_groups`/APTED("qualified_name") partially matching
pieces of an import path that should have been deleted wholesale with its now-unused function) - a
false-positive problem neither the chain-dispatch fix nor a delete-forbidden/insert-forbidden
license would touch. The `qualified_name`-tagged bucket (40 fixtures) is the one worth a second,
separate look later, since that tag means an earlier pass *actively chose* a mapping contradicting
ground truth, unlike `fast_fallback`'s "nothing upstream claimed this" default. `rust_add_to_
existing_use`: `scoped_identifier` (before) vs. `scoped_use_list`/`use_list` (after) - a genuine
grammar-level shape change (single import becoming a multi-import list) - kept as the named exemplar
for the *smallest* target category, now confirmed to be a minority of the remaining gap (4/121
fixtures show an explicit kind mismatch).

**Conclusion**: Phase 3b is now scoped as a *region-finding dispatcher* over already-working
machinery (bounded APTED confirmed above; the constrained-LCS resolver Phase 3a's second half would
have built has no demonstrated customer - 65/72 licensed fixtures are already zero, and the 7
residual ones are a different shape), not a "build a new resolver" phase. Concretely: find maximal
same-kind repeat-chains and same-shaped mixed regions still unmatched after the propagation/
fallback pipeline, size-gate them, and route to `apted::for_nodes` instead of the fallback's
whole-subtree-hash Myers LCS for those regions specifically - the `resolve_forest` chokepoint the
original plan named is still the right place. The `qualified_name` over-matching bucket and the
`none_mapped`/`css-add-property`-shaped empty-block cases are separate, smaller follow-ups, not
part of this phase.

### Phase 3b result: single-entry-gap real-APTED dispatch (branch commit follows)

Implemented directly in `resolve_residual_forest_via_myers_lcs` (`apted/common.rs`) rather than a
new dispatcher module: after its existing exact-hash `myers_lcs` pass over `maximal_unmatched_
roots`, split the leftover unpaired roots into anchored segments (reusing `split_into_anchored_
segments`, the same "diff each gap between confirmed anchors independently" idiom `resolve_flat_
tree_pair` already uses for one parent's flat children). A segment gets a real, bounded
`apted::for_nodes` call instead of atomic delete/insert.

**First attempt regressed 10 fixtures** (one 0->32, `kotlin-refactor-function`) by pooling up to
`FLAT_UNMATCHED_RECURSE_LIMIT` (20) entries per side into one `resolve_forest` call, mirroring
`resolve_flat_tree_pair`'s own leftover recursion. Root cause: unlike that function's entries (real
ordered siblings under one shared parent), this residual's maximal-unmatched-roots are scattered,
*unrelated* fragments from anywhere in the file - given a multi-candidate pool, APTED confidently
invented a plausible-but-wrong cross-match between an unrelated deleted function's descendants and
merely-similar nodes elsewhere in the same gap. **Fixed by restricting to exactly one entry per
side** - a true, unambiguous 1:1 "this replaced that" correspondence between two confirmed anchors,
with no room for APTED to invent a relationship. Still bounded by a 2000-node-per-side total-size
cap (`RESIDUAL_SEGMENT_MAX_TOTAL_SIZE`) on top of the 1-entry restriction, since a lone entry can
still be an arbitrarily large subtree.

Full 339-fixture corpus, isolated to this fix: **zero-mismatch 217 -> 243 (64.0% -> 71.7%)**, total
mismatches 5052 -> 4131, only **1 minor regression** (`vimscript-neovim-neovim-add-two-functions-
and-modify-a-few-lines`, 53 -> 58 - the same small residual cross-matching risk the 1-entry
restriction reduces but can't eliminate entirely, since even a "true" 1:1 gap pairing isn't
*guaranteed* correct, just far more constrained than an open pool), 52 improvements. Latency flat
(p50 119.7 -> 114.7ms, p99 9741 -> 9575ms, both noise). Confirms the exemplar fixture directly:
`javascript-microsoft-typescript-broken-js-remove-string-fragment` 602 -> 464 (not fully zero - a
602-mismatch fixture has more than one affected chain, and only single-entry gaps qualify; some of
its chains apparently sit in multi-entry gaps). Bonus: `css-add-property`'s empty-`{}`-block
residual (previously assumed to be a different, unexplained shape) also resolved to 0 - it turned
out to be the same single-entry-gap case, not a distinct category.

Test suite: 58 -> 18 failing (`cargo test --release --features test-fixtures --lib`). 71.7% is now
within reach of the pre-Phase-1 baseline (74.3%) and closing in on the 90% target - remaining gap is
concentrated in the `qualified_name` over-matching bucket (separate, not yet investigated) and
fixtures whose affected chains span multi-entry gaps (need real disambiguation to recurse safely,
not just a size increase on the entry cap - the exact risk this phase's first attempt hit).
`research/data/quality/optimal_solutions_benchmark.csv` refreshed to this result.

### Speed investigation: `flat_children`'s gate, revisited (2026-08-16)

User asked "what is our speed" - checked against the p50<100ms/p99<400ms goal. Answer at the time:
p50 114.7ms (close), p90 1.3s, **p99 9.6s (24x over target), max 177s** - two fixtures
(`rust-real-logic-change-in-a-huge-75k-node-file` 177s, `tsx-excalidraw-...-huge-file` 150s)
dominated the tail; nothing speed-related had been touched since Phase 1.

Profiled both directly (temporary phase-by-phase timing, reverted after use - see this section's
methodology note below): **~95% of both fixtures' time was in `solve_large_flat_subtrees`**
(23.2/24.2s and 16.6/17.4s respectively). One level deeper: one single top-level item each time (a
`mod tests { ... }` with a ~30K-node flat body; similar for excalidraw) accounted for nearly all of
it. Root cause, precisely identified: `flat_children`'s gate (deciding whether a container gets the
cheap Myers path or falls through to general Zhang-Shasha/APTED) checked *unmatched* child count
against `FLAT_MIN_CHILDREN` (50), not total child count - a container with 30K total children but
only ~20 still-unmatched (the rest already hash-matched by Phase 1) falls *below* 50 unmatched and
gets routed to general tree-edit-distance over the whole container instead of Myers.

**This is not a new bug - it's a known one, previously investigated and reverted.** Memory record
`flat_children-gate-widening-attempt-2026-08-14` (and this file's own history) documents an
identical diagnosis and fix attempt on 2026-08-14: widening the gate to total count fixed the exact
pathology (confirmed then too: "18.5s of one fixture's 28s total") but broke 9 corpus fixtures,
because `resolve_flat_tree_pair`'s own leftover-recursion (`FLAT_UNMATCHED_RECURSE_LIMIT`, an
*entry count* cap, no size cap) was too weak for containers whose newly-exposed residual needed
general APTED - and was reverted for lack of a fix that didn't weaken that path. That gap is
exactly what Phase 3b's `resolve_residual_forest_via_myers_lcs` work (this same session, above)
had just built and validated: a total-*size* cap alongside the entry-count cap, since tree-edit-
distance cost is driven by node count, not entry count.

**Re-attempted with that fix in hand**: (1) widened `flat_children`'s gate to total child count
(removing its now-unused `node_map` parameter); (2) added `FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE`
(2000, same value/reasoning as Phase 3b's `RESIDUAL_SEGMENT_MAX_TOTAL_SIZE`) to `resolve_flat_tree_
pair`'s leftover recursion. First measurement: fixed both target fixtures (23.9s -> 3.4s and
17.4s -> ~2s of `solve_large_flat_subtrees` time) and net-improved the corpus (4131 -> 3255 total
mismatches after the full fix lands - see below), but introduced 5 new regressions, one of them a
full 0 -> 343 flip.

**Diagnosed rather than accepted**: added temporary per-call instrumentation (reverted after use)
to `resolve_flat_tree_pair`'s leftover recursion, capturing entry counts and total size for the two
worst-behaved fixtures. `xml-odoo-odoo-add-two-attributes` (0 -> 160 regression): exactly **1
before-entry, 1 after-entry, 17,670 combined nodes** - a true, unambiguous 1:1 correspondence,
blocked purely by the size cap, no cross-matching risk possible (nothing else to choose between).
`tsx-excalidraw-...-huge-file` (the fixture that motivated the cap in the first place): **6
before-entries, 12 after-entries, 21,056 combined nodes** - a genuine multi-candidate pool with
mismatched counts, exactly the shape where an unbounded pooled real-APTED call can invent a
plausible-but-wrong cross-match (confirmed separately: removing the cap entirely made this fixture
*worse* on both quality and speed, 1458 mismatches/19.3s vs. 231/2.4s capped - not just slower).

**Fix**: exempt the size cap specifically for the exactly-one-entry-per-side case (mirroring Phase
3b's own single-entry-gap reasoning exactly) while keeping it for genuine multi-entry pools. This
resolved `xml-odoo-odoo-add-two-attributes` (0 mismatches) without reopening the excalidraw risk
(stayed at 231). Final result, full 339-fixture corpus: **zero-mismatch 243 -> 240 (71.7% ->
70.8%)**, total mismatches 4131 -> 3255, **4 remaining regressions** (all confirmed or
strongly-suspected multi-entry-pool cases the cap is deliberately still blocking - `java-nextcloud-
android-add-if-branch-with-reused-return` 54->343, `java-nextcloud-android-add-two-function-calls`
0->30, `json-excalidraw-...-change-translations-mostly-add` 0->27, `java-protobuf-add-two-
annotations` 0->5) vs. 1 huge improvement (`tsx-excalidraw-...-huge-file` 1458->231, the fixture
this whole investigation started from). Test suite: 18 -> 21 failing, consistent with the 3-fixture
net zero-mismatch delta.

**Latency, the actual point of this investigation**: **max 177,105ms -> 11,386ms (15.6x)**, p99
9575ms -> 7621ms, p50/p90 unchanged (114.7->115.3ms, 1288->1265ms, both noise). Corpus-wide
benchmark wall time (339 fixtures x 3 diff computations each) 1997s -> 756s (2.6x). Neither
originally-targeted fixture is in the top 5 slowest anymore except `rust-real-logic-change-in-a-
huge-75k-node-file` (177s -> 10.6s - still the second-slowest fixture in the corpus, now bounded by
something other than `solve_large_flat_subtrees`, not yet investigated). p99/max are still well
over the 400ms target - this investigation closed the two worst outliers, not the whole tail; the
next-slowest fixtures (`json-iwalton3-jellyfin-web-...` 11.4s, two more `rust-rustdesk-rustdesk-...`
fixtures at 8-9s) are now the frontier and haven't been profiled.

**Methodology note**: every diagnostic in this section (phase-by-phase timing in `PendingDiff::
finish`/`pending_with_config`, per-item timing in `solve_large_flat_subtrees`, per-call
entry-count/size logging in `resolve_flat_tree_pair`) was added as temporary instrumentation
(`eprintln!` gated behind a `CODEDIFF_DEBUG_*` env var, or a throwaway `#[ignore]`d test) and
reverted immediately after use via `git checkout --` once it had answered its question - none of it
shipped. Same discipline as the debug tests used earlier in this session (Phase 0's `hunk_census.
rs`, the propagation investigation) - confirmed clean via `git status`/`git diff` before every
commit in this section.

### Quality push: generalize the single-entry-gap exemption to equal-count gaps (2026-08-16)

User asked for a direct `main` vs. branch comparison to gauge readiness. Result: branch was
**behind** main on the metric that actually gates a merge - zero-mismatch 70.8% (branch) vs. 74.3%
(main), 23 fixtures regressed vs. main against 18 improved - even though total mismatches (3255 vs.
21109) and latency (max 11.4s vs. 192s, p99 7.6s vs. 43.1s) were dramatically better. Investigated
all 23 regressions: most showed the same `reason=APTED("fast_fallback")` signature as the exemplar
`resolve_residual_forest_via_myers_lcs` was built around, suggesting the single-entry-only
restriction (Phase 3b, above) was too conservative for gaps with more than one leftover entry.

Generalized `resolve_residual_forest_via_myers_lcs`'s recursion from "exactly one entry per side"
to **any gap with equal counts on both sides**, still never pooled - each `before_seg[i]` is
recursed against only `after_seg[i]` (fixed document-order position within the gap), one
independent `apted::for_nodes` call per pair, not one `resolve_forest` call over the whole gap.
This preserves the exact safety property the single-entry restriction was for (each call only ever
sees one candidate per side, so APTED has nothing to cross-match against) while covering "N
separate small edits landed in the same gap" - exactly the shape `vimscript-neovim-neovim-add-two-
functions-and-modify-a-few-lines` and `css-mozilla-firefox-firefox-actual-style-changes` turned out
to be. Gaps with *unequal* counts (a real insert/delete inside the gap too) still fall back to
atomic delete/insert unchanged - no fixed positional correspondence to exploit there without
reintroducing the pooling risk.

One test (`resolve_residual_forest_via_myers_lcs_replaces_everything_past_the_edit_cap`, 600
same-count leaves each side) broke on the first attempt - same root cause as the earlier Phase 3b
test fix: its `leaf()` helper nodes are all `text: String::new()`, so `UnitCostModel::ren` (which
compares `kind`/`text`, never `node_to_full_hash`) couldn't tell them apart and treated same-
position pairs as free relabels. Fixed by giving the test real per-position distinct kind/text
(`leaf_with_kind`) rather than relying on hash alone - with genuinely different content the
mechanism correctly still deletes+inserts every pair (confirms the design is safe for truly
unrelated same-position content, not just convenient on this specific test).

Full 339-fixture corpus, isolated to this fix: **zero regressions, 4 improvements**
(`css-mozilla-firefox-firefox-actual-style-changes` 65->20, `vimscript-neovim-neovim-add-two-
functions-and-modify-a-few-lines` 58->22 - fully recovered to its pre-flat_children-fix value -
`lua-awesomewm-awesome-change-doccomments` 39->21, `css-wordpress-wordpress-autogenerated-file`
16->0). Zero-mismatch 240 -> 241 (70.8% -> 71.1%), total mismatches 3255 -> 3140. Latency
unaffected (per-pair calls are individually bounded the same way the single-entry case always was).
`vimscript-neovim-neovim-improved-asserts` (0->53 vs. main) investigated but unchanged - its gap is
evidently not equal-count, a different shape needing separate diagnosis. `research/optimal_
solutions_benchmark.csv` refreshed to this result.

Still short of main's 74.3% zero-mismatch bar (71.1% now, was 70.8%) - the remaining gap includes
`vimscript-neovim-neovim-improved-asserts`'s unequal-count-gap shape, the `qualified_name`
over-matching bucket (still not investigated), and whatever's behind the other ~19 remaining
regressions vs. main not yet individually diagnosed.

### Quality push, continued: `resolve_flat_tree_pair`'s pooling had the same risk (2026-08-16)

Extended the equal-count-vs-pooled distinction to `resolve_flat_tree_pair`'s own leftover
recursion (the flat-container path, not the residual-forest path above) - it previously pooled
*any* leftover up to `FLAT_UNMATCHED_RECURSE_LIMIT` (20) entries unconditionally, on the assumption
(stated in its own doc comment) that its entries are "genuine ordered siblings under one shared
parent" and therefore safe to pool. **That assumption was wrong, confirmed directly, not just
theoretically**: `java-nextcloud-android-add-if-branch-with-reused-return` (54 -> 343 vs. main) has
a leftover of just **2 entries each side** - nowhere near the old cap - and pooling still let APTED
invent a plausible-but-wrong cross-match between two unrelated methods, deleting one wholesale. The
same risk the residual-forest fix (above) demonstrated at larger scale exists here too, even at
N=2.

Rewrote the dispatch: equal-count gaps now always recurse per position (`before_unmatched[i]`
against only `after_unmatched[i]`, one independent call per pair, no entry-count cap needed since
each call is bounded on its own) instead of pooling; unequal-count gaps keep the original pooled
`resolve_forest` call (bounded by both the entry-count and total-size caps) as a deliberate,
validated risk/reward trade with no safer alternative. First attempt at this dropped the
single-pair-exempt-from-size-cap carve-out from the earlier fix along the way, regressing
`xml-odoo-odoo-add-two-attributes` right back to 160 - caught by the same full-corpus measurement
discipline, restored (`N == 1` is exempt from the size cap regardless of magnitude; `N > 1`
equal-count is still size-capped, since per-pair cost still needs *some* latency bound even without
a correctness risk).

Full 339-fixture corpus, isolated to this fix: **zero regressions, 3 improvements**
(`java-nextcloud-android-add-if-branch-with-reused-return` 343->54, back to exactly main's value;
`java-protobuf-add-two-annotations` 5->0; `c-ffmpeg-added-typedef-to-enum` 2->0). Zero-mismatch
241 -> 243 (71.1% -> 71.7%), total mismatches 3140 -> 2844. `research/optimal_solutions_benchmark.
csv` refreshed to this result.

Running tally this session, `main` (74.3%) vs. branch: 74.3% -> ~21% (Phase 1) -> 64.0% (Phase 2/3a
bug fix) -> 71.7% (Phase 3b) -> 70.8% (flat_children speed fix, deliberate trade) -> 71.1% (equal-
count residual-forest generalization) -> 71.7% (equal-count flat-tree-pair generalization,
matching Phase 3b's post-fix level).

### Quality push, continued: the total-size cap on per-position pairs was pure overhead (2026-08-16)

Re-diagnosed the next regression by the same recipe: `java-nextcloud-android-add-two-function-
calls` (0 -> 30 vs. main) turned out to be an equal-count gap too (2 entries each side), just over
`FLAT_UNMATCHED_RECURSE_MAX_TOTAL_SIZE` (5,454 combined nodes) - blocked by the size cap that
carried over from the pooled design into the new per-position one. Re-examined whether that cap
still made sense there: it exists to bound a *pool's* cost (which genuinely scales with combined
size, since it's one dense computation over every candidate at once), but a per-position pair's
cost is independent of every other pair - nothing to cross-match against regardless of size, so
capping it was never protecting against a correctness risk the way it does for the pooled/unequal-
count branch, only an unconfirmed latency one. Tested directly rather than assumed: removed the cap
for the equal-count branch entirely. Result - **zero regressions, 1 improvement**
(`java-nextcloud-android-add-two-function-calls` 30 -> 0), zero-mismatch 243 -> 244 (71.7% ->
72.0%), latency unmoved (p99 7797 -> 8011ms, max 11880 -> 11554ms, both noise). Kept uncapped;
the size cap now applies only to the pooled unequal-count branch, where it's still load-bearing.

Running tally: **72.0%**, 2.3 points short of main's 74.3%. Remaining known gap: `vimscript-neovim-
neovim-improved-asserts` (unequal-count shape, not yet re-diagnosed against this session's fixes),
the `qualified_name` over-matching bucket, `cpp-add-templates`/`rust-turbopack-module-rule`-class
wrap/reparent cases (a structural-shift shape none of this session's mechanisms address), and an
unknown number of the remaining ~17-19 regressions vs. main not yet individually diagnosed - worth
re-running the full main-vs-branch diff before continuing, since several may already be fixed as a
side effect of these fixes (this has been true at every step so far).

### Quality push, continued: same size-cap-removal applied to the residual-forest sibling (2026-08-16)

Applied the just-validated finding (removing the size cap from a per-position branch is safe,
since a per-position pair has nothing to cross-match against regardless of size) to `resolve_
residual_forest_via_myers_lcs`'s own equal-count branch too - it had carried the same `RESIDUAL_
SEGMENT_MAX_TOTAL_SIZE` cap since Phase 3b, for the same now-outdated reason. Removed
`RESIDUAL_SEGMENT_MAX_TOTAL_SIZE` entirely (dead once its only caller's cap was lifted).

Full 339-fixture corpus: **zero regressions, 2 improvements**
(`vimscript-fedorenchik-qt-support-add-two-lines` 19 -> 0, `vimscript-neovim-neovim-awful-test-
case-bunch-of-hex-colours-more-data-than-code` 22 -> 11). Zero-mismatch 244 -> 245 (72.0% -> 72.3%),
total mismatches 2814 -> 2784, latency flat (p99 8010 -> 7906ms, max 11554 -> 11699ms, both noise).

Running tally: **72.3%**, 2.0 points short of main's 74.3%. The two size-capped branches
(`resolve_flat_tree_pair`'s and `resolve_residual_forest_via_myers_lcs`'s equal-count recursions)
are now both uncapped and share one lesson worth remembering going forward: a size cap that made
sense for a *pooled* multi-candidate call doesn't automatically transfer to a *per-position*
call built to replace it - re-derive whether a cap is still protecting against a real risk before
carrying it over, rather than assuming.

### Dead-code removal: the two permanently-disabled solver knobs (2026-08-16)

Part of the planned "Phase 5: cleanup" work, pulled forward on request rather than deferred: both
`solver_import_nodes` (the normalized-import-path hash variant in `solve_hash_descent`) and
`solver_bottom_up_expansion` (the Dice-threshold pass, gating what used to be phases 3 and 5) had
been `false` by default since the 2026-07-15 ablation study found each net-negative in isolation,
with no re-measurement since. Removed outright rather than left as always-off toggles: deleted
`solve_bottom_up_expansion.rs` entirely, removed `solve_import_path_hash` and its helpers from
`solve_hash_descent.rs`, dropped both `HeuristicConfig` fields and their `benchmark_optimal_
solutions` CLI ablation flags, and removed the now-unreachable `ASTMappingReason::BottomUpExpansion`
/`NormalizedImportPath` variants. `solve_bottom_up_propagation` (the *different*, currently-enabled
mechanism that replaced `solve_bottom_up_expansion`'s conceptual slot - similar name, unrelated
code, see the session's own end-to-end pipeline explanation for the distinction that prompted this)
is untouched.

Verified byte-identical, not just "should be fine": full 339-fixture corpus, zero fixtures'
mismatch count changed. Expected, since both removed paths were already unreachable in every
production/default configuration - this was pure surface-area reduction, not a behavior change.

### `fast_fallback` unequal-count residual gaps: kind-only sub-anchoring (2026-08-16)

The "one final push" quality work's last target: `resolve_residual_forest_via_myers_lcs`'s
unequal-count segments (`src/diff/apted/common.rs`) fell through unconditionally to atomic
delete-all/insert-all, even when a segment plainly contained real reuse (e.g. a 3-before/2-after
gap where two of the three genuinely correspond to two of the after-side entries). Sampled via a
temporary `CODEDIFF_DEBUG_LEFTOVER` instrumentation against the `fast_fallback`-flagged fixture
bucket: confirmed this bucket's shape is real and reasonably uniform (small, unequal-count leftover
gaps), and 9 of the 35 flagged fixtures sat at ≤3 mismatches - real headroom toward the 245→252
zero-mismatch gap vs. `main`.

**Shipped**: `resolve_unequal_segment_via_kind_only_anchors` - a second, finer `myers_lcs` pass over
the segment's `node_to_kind_only_hash` values, with matched pairs recursed per-position (one
candidate per side, same no-pooling guarantee as the equal-count branch). Landed with two safety
filters found necessary only after real regressions, not designed in up front:
- `KIND_ONLY_ANCHOR_MIN_SIZE` (50): `KindOnlyHash` never hashes leaf values, so small/shallow
  subtrees collide on shape alone regardless of actual content. Two regressions surfaced this at
  different scales - a same-kind `comment` **leaf** collision (`swift-swiftlang-swift-enable-
  checks-remove-todo-comment`, 2→4) and, more surprisingly, a size-11 **non-leaf** `element`
  collision in repeated HTML markup (`html-gohugoio-hugo-enclose-table-with-div-and-add-thead-
  tbody`, 9→27; `css-mozilla-firefox-firefox-actual-style-changes` regressed the same way at size
  14). The one confirmed genuine win (`vimscript-neovim-neovim-improved-asserts`, 53→0) anchored at
  size 186, over 10x either false positive - a size floor at 50 cleanly separates them.
- Segment-local hash-value uniqueness (only trust a pair if neither side's hash repeats elsewhere in
  the same segment) - a second, independent safety net for the ambiguous-candidate case, though it
  did **not** catch the HTML regression above (that pairing was already unique within its segment;
  the true correspondent simply wasn't present in the segment at all - see below).

**Key negative result, worth not re-deriving**: `KindOnlyHash`'s safety in phase 1
(`solve_hash_descent.rs`) doesn't come from the hash being inherently safe - it comes from phase 1's
node selector (`reference_nodes_ordered`, large declaration-level nodes only, evaluated across the
*whole file*) restricting what's eligible to hash in the first place. Reusing the same hash locally,
inside one small residual segment, loses that guarantee: repeated/templated structures (HTML rows,
similarly-shaped statements) can share a `KindOnlyHash` while being genuinely different content, and
no purely local filter (leaf exclusion, segment uniqueness) catches the case where the wrong
same-shape candidate is unique *within the segment* but wrong anyway. A size floor is a heuristic
that happened to separate the one real win from the two known false positives on this corpus - it is
not a structural guarantee, and a future session extending this bucket further should treat any new
low-size win with suspicion rather than assuming the mechanism generalizes safely downward.

**Verified**: full 339-fixture corpus, zero regressions, one fixture flipped to zero
(`vimscript-neovim-neovim-improved-asserts`, 53→0). 245/339 (72.3%) → 246/339 (72.6%) zero-mismatch,
still short of `main`'s 252/339 (74.3%) baseline this branch must clear before merging.

**This branch has since been fast-forward merged to `main`** and the corpus has grown to 417/418
fixtures (many new human solutions added). Current baseline as of 2026-08-17: 310/418 (74.3%)
zero-mismatch, p50 66ms/p90 781ms/p99 4.69s/max 6.9s - still well short of the 90%-zero-mismatch and
p99<400ms targets. Re-scoping below picks up from here, not from the 246/339 figure above.

### Task #7 ("Phase 3c") re-scoped, then re-scoped again (2026-08-17)

The plan's original Phase 3c (ERROR-density gate + leaf-hash refinement, kill criterion pinned to
`css-shadcn-ui-ui-completely-broken-treesitter-parsing` dropping from 16277) turned out to already be
solved - that fixture was down to 124 as a side effect of the `maximal_unmatched_roots` traversal fix
above, not anything Phase 3c would add. Re-scoped toward "multi-entry-gap chain dispatch" based on a
stale reading of this file's own Phase 3b writeup (which flagged multi-entry gaps as an open tail) -
but a fresh read of the code (`resolve_residual_forest_via_myers_lcs`, `apted/common.rs`) showed
equal-count multi-entry gaps are already handled (per-position recursion, uncapped, 2026-08-16), and
the 464-mismatch fixture originally cited as the target (`javascript-microsoft-typescript-broken-js-
remove-string-fragment`) turned out to be 464/464 `IdenticalHash`/`IdenticalHashOfAncestor` - a
phase-1 hash-collision problem, unrelated to residual dispatch. Both corrections surfaced to the user
before continuing, rather than silently substituting a different mechanism under the same task label.

A full bucket breakdown across all 107 then-nonzero fixtures (dominant `reason` tag per fixture, via
`--details`) found the `fast_fallback` bucket (37 fixtures, 466 mismatches) is itself at least three
unrelated failure shapes, not one: (1) duplicate-hash Myers tie-break drift in `css-shadcn` (124
mismatches, confirmed via temporary `CODEDIFF_DEBUG_RESIDUAL` instrumentation - only ~244 entries are
left in leftover segments after the top-level pass, so the 124 mismatches are wrong matches picked
*inside* the ~16,283-entry top-level `myers_lcs` call itself, where thousands of near-identical
`ERROR`/leaf tokens collide on hash; same class as two known regressions this session, high risk to
touch further, not pursued), (2) wrap/reparent (`cpp-add-templates`, fixed below), (3) moved/
reparented chains (`html-apache-echarts-actual-structure-change`, `rust-turbopack-module-rule` - the
already-documented moved-code gap). User chose (2) after this breakdown.

### Fix: trivial-entry filtering unlocks wrap/reparent matches (2026-08-17)

Root cause, found via the same `CODEDIFF_DEBUG_RESIDUAL` instrumentation: `cpp-add-templates`'s
residual gap was `before_seg=[class_specifier(49), ;(1)]` vs `after_seg=[template_declaration(58)]` -
a genuine wrap (`class_specifier` gets a new `template_declaration` parent) sitting in an
unequal-count (2 vs 1) segment purely because of an unrelated size-1 `;` in the same gap. Unequal
counts fall to `resolve_unequal_segment_via_kind_only_anchors`, which compares whole-subtree
`KindOnlyHash` values - `class_specifier` and `template_declaration` have different kinds, so they
never hash-match there regardless of the size floor, and the whole segment fell through to atomic
delete/insert.

Fix: before the unequal-count branch, filter entries with `node_to_subtree_size <=
TRIVIAL_ENTRY_MAX_SIZE` (started at 1 - matches true leaves and entries missing size data, nothing
larger) from both sides and re-check for equal counts among what's left. If they match, recurse the
substantial entries per-position through real APTED (same safety argument as the existing
equal-count branch - each is still the only candidate at its document-order position, so there's no
room for APTED to invent a cross-match) and resolve the filtered-out trivial entries independently
via plain delete/insert, same as an unmatched leaf would get anyway. `cpp-add-templates`: 25 → 2
mismatches (the residual 2 are the root's own `MatchButNotIdentical`, needing one more propagation
pass, and the `;` itself, which is one of the filtered trivial entries and so wasn't matched to its
new nested position).

**Verified**: full 418-fixture corpus, **zero regressions**, 3 improvements: `cpp-add-templates`
(25→2), `css-mozilla-firefox-firefox-actual-style-changes` (20→8), `typescript-async-await` (9→6) -
the latter two turned out to share the same trivial-leaf-alongside-a-real-wrap shape, not solely
their previously-documented explanations. This branch runs *before*
`resolve_unequal_segment_via_kind_only_anchors`, so it changes which mechanism handles any segment
where trivial-filtering makes counts equal - `css-mozilla-firefox-firefox-actual-style-changes` was
specifically named in `KIND_ONLY_ANCHOR_MIN_SIZE`'s own doc comment as a regression case from that
other mechanism, so its improvement here was double-checked, not just trusted from the aggregate
count: diffed its full `--details` output line-by-line against a pre-fix build (`git stash` +
rebuild, the same isolation technique used earlier this session) - the remaining 8 mismatches are a
byte-for-byte **subset** of the original 20 (the fixed 12 are three whole `declaration` groups; the
untouched 8 are an unrelated `media_statement` deletion and an `integer_value`/`plain_value`
ambiguity, unchanged from before), confirming this is a clean improvement, not a masked new wrong
match. Zero-mismatch count unchanged (310/418, none of the three hit exactly zero) but total
mismatches dropped. Latency deltas were all under 2x on already-largest fixtures (this project's own
established threshold for "don't trust a single unrepeated `elapsed_ms` run as signal" - see the
Phase 0 findings above) - not treated as a regression signal. `research/optimal_solutions_benchmark
.csv` refreshed to this result.

`TRIVIAL_ENTRY_MAX_SIZE` started at 1 deliberately, matching only the observed case - per
`KIND_ONLY_ANCHOR_MIN_SIZE`'s own history (see the kind-only sub-anchoring section above), widening a
size-based trust threshold without new corpus evidence has caused real regressions before in this
exact area of code. A future session chasing more of this bucket should re-verify on the full corpus
before raising it, not assume it generalizes.

**`css-shadcn-ui-ui-completely-broken-treesitter-parsing`'s 124 mismatches are an explicit accepted
residual, not an unexamined gap**: diagnosed above (in the re-scoping note) as duplicate-hash Myers
tie-break drift across ~16,283 near-identical `ERROR`/leaf tokens in the top-level
`resolve_residual_forest_via_myers_lcs` pass itself, not a dispatcher gap. It is currently the single
largest nonzero fixture in the corpus. Deliberately not pursued this session: fixing it safely would
need a genuinely new disambiguation signal (positional or content-aware tie-breaking), and this exact
class of fix has caused two real regressions elsewhere in this file already
(`KIND_ONLY_ANCHOR_MIN_SIZE`'s doc comment). A future session should re-derive this from the code, not
assume the mechanism is safe to extend, before attempting it.

### Task #8 ("reposition move-detection") investigated, no customer found (2026-08-17)

Checked whether calling `solve_moved_subtrees::solve` a second time, before `apted::for_roots_fallback`
(not just its current dead-last position), would rescue moves the fallback fragments before move
detection's strict `subtree_fully_unmapped` guardrail ever sees them. Tested directly on
`rust-turbopack-module-rule` - `solve_moved_subtrees.rs`'s own named "moved code" example, currently
misdiagnosed with 49 `fast_fallback` mismatches - via temporary instrumentation inside `solve_moved_
subtrees` (added, used, fully reverted - `git status` clean afterward). **Result: negative.** The
candidate `match_arm` subtrees have zero full-hash candidates anywhere in the after-tree - they were
never fragmented by the fallback, they're genuinely different text (old `Display::fmt`'s match arms
vs. new `ConfiguredModuleType::parse`'s, same patterns, different bodies). This needs pattern/
similarity-based alignment, not hash-based move detection, regardless of pipeline position. Broader
sweep: of 107 nonzero fixtures, only 6 have any `MovedSubtree` mapping at all, and only one of those
(`xml-odoo-odoo-add-button-roles`) also has `fast_fallback` mismatches in the same fixture - and its
shape (`StructurallyIdenticalAncestor`-dominated, ambiguous duplicate XML content) doesn't match the
fragmentation pattern either. **Conclusion**: calling move detection twice is safe by construction
(same idempotent-append reasoning that already justifies the double `solve_bottom_up_propagation`
call - it only ever converts `target==0` pairs, never steals from a real mapping) but has no
demonstrated customer in the current corpus. Not implemented. A future session should look for direct
evidence (a fixture with `MovedSubtree` mappings *and* `fast_fallback` deletes concentrated in the
same subtree) before building this on spec.

### `qualified_name` bucket surveyed, two different hard problems found, neither fixed (2026-08-17)

39 fixtures (the corpus's largest bucket by fixture count, ~600 total mismatches, top 4 alone =
279) have `APTED("qualified_name")` as their dominant mismatch reason - `solve_qualified_name_groups`
(`solve_syntax_aware_matching.rs`) matches whole functions/methods/etc. by fully-resolved name, then
runs a real, bounded `apted::for_nodes` call scoped to that one pair; the reason tag marks everything
decided *inside* that real APTED call, not a coarse dispatch mechanism like `fast_fallback`.

Initial surface read of the top 2 fixtures' mismatch paths looked like the same wrap/reparent shape
`TRIVIAL_ENTRY_MAX_SIZE` just fixed (a `let_condition`/`elseif_statement` appearing on only one
side's path). **Directly checking the fixtures' actual source diffs disproved this** for both:
- `lua-neovim-neovim-if-flips-two-branches` (68 mismatches): the real edit is two `elseif` branches
  **swapping position** (`if A ... elseif B` → `if B ... elseif A`, each branch's own content
  unchanged) - a commutative/order-independent sibling matching problem, not a reparent. This is
  literally item #1 on this file's own "Phase 1: Quick Wins" list below ("Commutative Sibling
  Matching"), never implemented.
- `rust-zed-workspace-tasks` (91 mismatches): not a clean unwrap either - `--dump` showed the
  mismatched `let`/`.` tokens matched to unrelated positions deep inside a large genuinely-new
  block (a `SaveStrategy` match, relocated spawn logic), not the reparented condition. The real
  source diff confirms a substantial rewrite where some old content survives deep inside new
  structure - a search-quality/cost-model gap (the "search failure" bucket already characterized in
  "Architecture rethink: target goals" above), not a single fixable mechanism.

**Conclusion**: unlike `fast_fallback`, this bucket is not one mechanism with one fix - it's at least
two different hard problems (commutative sibling reordering: scoped, real, but a nontrivial feature,
not a quick filter; general search-quality gaps in large rewrites: no clean fix available, needs the
broader cost-model/search work). Not attempted this session - flagged for scoping, not fixing, if
picked up again. A future session should not assume the remaining 37 fixtures are evenly split
between these two shapes without checking a few more source diffs directly (path-string inference
alone was actively misleading on both examples checked here).

### Literature + external-implementation survey, cross-checked against corpus stats (2026-08-17)

Two new corpus-wide analyses landed this session, independent of everything above: `analyze_human_
mappings`'s node-instance operation mix (`node_op_*` columns, `research/data/quality/human_mapping_analysis.csv`)
and a "shape of human solutions" chart (`research/analysis/human_mapping_shapes_report.py`) showing
each fixture's change composition, normalized to its own non-Identical mass. Cross-referencing
fixture *typology* (which operation dominates a fixture's changed nodes) against `current_mismatches`
found a real, non-uniform pattern: fixtures dominated by `InsertWithChildren` have by far the worst
mean mismatch count (15.5, vs. 4.6 for `MatchButNotIdentical`-dominant and 5.8 for `Mixed`), driven by
a handful of severe outliers -
`tsx-excalidraw-excalidraw-huge-file-with-real-logic-change` (231), `rust-zed-workspace-tasks` (117),
`cpp-ladybird-refactor-variables-if-changes` (109). `MatchButNotIdentical`-dominant fixtures (172/417,
the largest single group) have the *best* zero-mismatch rate (86.0%) despite being the most common
shape - matching succeeded, only content differs. `Mixed`-typology fixtures fare worse (57.8%
zero-mismatch) than either dominant-single-operation group. This independently re-derives several
fixtures already named above (`rust-zed-workspace-tasks`, `lua-neovim-neovim-if-flips-two-branches`)
from a completely different signal (corpus-wide typology vs. individual mismatch-path reading),
which is a useful cross-check that both methods are finding the same real problems, not noise.

Prompted by this, a literature/external-implementation search (not previously done for this specific
purpose - `related-works.md` surveys *tools*, not their internal heuristics) turned up three
candidates, cross-checked against both the corpus stats above and this file's own history so nothing
below re-suggests something already tried (see "Tried and rejected" entries throughout this file) or
re-describes something already shipped:

**1. GumTree Simple's bounded recovery phase** (Falleri & Martinez, ICSE 2024, "Fine-grained, accurate
and scalable source differencing") - replaces GumTree Greedy's O(n^3) TED-based recovery phase with a
three-step O(n^2) heuristic run recursively the instant a bottom-up mapping is found: (a) exact
subtree isomorphism among a newly-matched parent's unmatched children, (b) structural isomorphism
ignoring leaf *values* (so a renamed identifier still matches), (c) last-resort **unique-type
matching**: if a child node *kind* occurs exactly once among both sides' still-unmatched children of
the same matched parent, pair them - inspired by XYDiff. Measured ~99% fewer CPU-cycles than Greedy on
both benchmark datasets (GitHub Java, Defects4J); interestingly, it found *fewer* total mappings than
Greedy on GitHub Java's arbitrary large changes but *more* on Defects4J's small localized bug patches
- exactly this corpus's own shape (median fixture is 1.8% changed, per the density stats above).
**Status check against current `main`, not the stale assumption I started this investigation with**:
step (a) is `solve_hash_descent`, step (b) is already codediff's own `KindOnlyHash` sub-anchoring
(2026-08-16) - both shipped. The uncapped whole-residual real-APTED `final_pass` this paper's
complexity argument would also indict is *already gone* on `main` (`git merge-base --is-ancestor
phases-4-7-rearchitecture main` confirms that branch, including "Phase 1: remove whole-residual full
APTED, always use the cheap fallback" `bf79bfe`, is merged - the comment in `diff.rs` claiming this
"lives on the branch, not main" is stale documentation, not current behavior, and should be fixed).
**The one genuinely new, unimplemented piece is step (c), unique-type matching** - never attempted in
any form here. Low risk by construction (fires only when a kind is unique among both sides' unmatched
siblings under an already-matched parent - a narrow, precise trigger, not a broad heuristic).

**2. Scoped unordered/commutative sibling matching - correction after checking current `main`, not
just this file's old backlog text.** Full unordered tree-edit-distance is MAX-SNP-hard (Zhang et
al.), which is why no real tool solves it globally, but literature treats it as tractable when scoped
to one parent's children as a local minimum-cost assignment (permuting sibling order permutes the
assignment cost matrix without changing the optimal cost) or a per-parent Longest Increasing
Subsequence instead of positional Myers-LCS when order isn't semantically meaningful. **This file's
own "Commutative Sibling Matching" Phase-1 backlog entry just below, proposed as "sort children by a
canonical key before hashing," turns out to already be substantially shipped**: `nodes::is_
commutative_container` + `code::hash`'s `compute_kind_and_value_hash`/`compute_kind_only_hash` already
give order-independent hashing to every language construct that is *language-guaranteed* to be
order-independent - Rust `enum_variant_list`/`use_list`/`field_declaration_list`, Go/C#/Java/C/C++
enum and struct-field lists, JSON/YAML object keys, Python dicts, JS/TS objects, and more (see that
function's full match arm, `nodes.rs`). The backlog entry below is stale and should be updated or
removed, not re-implemented from scratch.

The fixture that actually motivates this section, `lua-neovim-neovim-if-flips-two-branches`, is a
**different, harder problem that `is_commutative_container` correctly does not attempt**: an
`if`/`elseif` chain is not language-guaranteed order-independent (reordering branches can change
program behavior for overlapping conditions) - a human made an order-changing edit to an
order-*sensitive* construct, and codediff cannot safely generalize "this kind is commutative" the way
it can for enum variants or struct fields. This is closer in spirit to `solve_moved_subtrees` (move
detection for content that relocated) than to `is_commutative_container` (blanket structural
order-independence) - what's actually needed is branch-level move detection scoped to a single
if-chain's clauses, tolerant of the bare-if-vs-wrapped-elseif grammar inconsistency (Lua's grammar
only wraps the *second-and-later* clause of an if-chain in `elseif_statement`; the first clause's
condition+body sit bare under `if_statement`, so even a same-kind-only comparison has nothing to
compare against for the first clause). **Not implemented this session** - needs its own short design
pass (what counts as "a clause" across the bare/wrapped grammar quirk, and how to bound the risk of a
move-detection-style heuristic firing on a genuinely order-changing edit that was NOT a simple swap)
before writing it.

**3. MinHash/LSH-based approximate candidate search** for the large-rewrite "search-quality gap"
(`rust-zed-workspace-tasks` and the `qualified_name` bucket's second failure shape above) - already
listed in this file's old Priority Reference table ("LSH for approximate matching") but never
elaborated or attempted. Important distinction from the *already-tried-and-reverted* "container
dissimilarity cost" experiment (2026-07-11, text-similarity surcharge on `UnitCostModel::ren`, broke 5
perfect fixtures): that was a *cost-model* change, scoring an already-committed match. LSH-based
candidate search is a *search/candidate-generation* technique - cheaply proposing plausible partners
before matching commits to anything - a different failure mode, so the prior revert doesn't
straightforwardly indict it, but this is the least-evidenced of the three candidates and would need
its own validation that the distinction actually holds up in this pipeline before trusting it.

**Decision**: implementing #1's unique-type-matching sub-phase next - lowest risk (narrow, precise
trigger condition), most clearly scoped, and a genuinely new mechanism rather than a variant of
something already tried. #2 needs a short design pass on the bare/wrapped-clause question first. #3
needs its own smaller validation before being trusted as different-in-kind from the reverted attempt.

### Latency baseline was ~5-6x harness artifact; honest numbers + the two real hotspots (2026-08-17)

Every `elapsed_ms` figure this file has ever recorded - including the "Architecture rethink" section's
corrected baseline (p50 123.9ms / p99 61.8s / max 169.3s) and every per-session A/B CSV - was
measuring a benchmark-harness artifact on top of true pipeline cost. Root cause:
`helper::handmade_test_code_pairs()` memoizes `Code` pairs and hands out **clones**, and `Code`'s
hand-written `Clone` deliberately drops `ast_metadata` to `None` (node ids are parse-specific, see
its doc comment). Every `metadata_of()` call in the pipeline then hits the silent
`Cow::Owned(compute_ast_metadata(...))` fallback and recomputes whole-tree metadata from scratch.
Measured directly (temporary env-gated instrumentation, added and fully reverted): **956
`compute_ast_metadata` calls totaling 32.2s for ONE benchmark row** on the largest fixture
(`json-iwalton3-jellyfin-web-...`, 258k nodes; ~26 full-file recomputes at ~245ms inside the single
timed `diff_code_with_config` call alone). The production path (`Code::from_string`/`from_file`)
precomputes metadata at construction and never pays this - the benchmark was overstating real diff
latency ~5-6x on any large fixture. This is the same hazard `handmade_test_code()`'s own 2026-07-26
doc comment describes ("20 separate compute_ast_metadata calls for one diff_code call") - fixed
there with `ensure_parsed()`, but `handmade_test_code_pairs()`'s clone-on-return path reintroduced
it for every benchmark consumer.

**Fix shipped** (`benchmark_optimal_solutions.rs`): clone + `ensure_parsed()` both sides per row
before any timed call, mirroring production. Verified: jellyfin 7.3s -> 1.0s; a residue of ~12
full-file + ~800 tiny recomputes per row remains inside the *untimed* mismatch/cost helpers
(`human_mapping` internals) - slows the benchmark run, doesn't touch `elapsed_ms`. A deeper
library-side fix (make `metadata_of` cache-on-first-use, e.g. interior-mutability `OnceCell`) is
open - any future caller that clones a `Code` and diffs it still silently pays ~26x.

**Honest baseline (fresh full-corpus run, idle machine, warm metadata, 2026-08-17)**: p50 **6.8ms**
(goal <100ms met ~15x over), p90 172ms, **p99 1.13s** (goal <400ms, ~2.8x over), max 4.9s, 23/417
fixtures over 400ms. Quality unchanged (2835 mismatches, 310/417 zero) - mismatch counts were never
affected. The p50/latency-architecture goals are in far better shape than believed; the whole
remaining p99 problem concentrates in **one pass**: phase-level timing on every fixture still over
~700ms shows `solve_syntax_aware_matching` at 95-99% of total diff time, split between exactly two
sub-passes:
- `solve_qualified_name_groups`: 1.4s on `kotlin-nextcloud-a-few-small-removals` (6.9k nodes!),
  1.3s on `typescript-excalidraw-...-add-values-to-lists` (6.2k), 727ms on
  `java-genymobile-scrcpy-refactor-for-loop` (3.0k), 2.7s on `rust-...-huge-75k-node-file`. Small
  files being slow means shape-driven blowup inside the per-name-pair bounded `apted::for_nodes`
  calls (or the group tie-break's cost estimation), not scale.
- `solve_large_flat_subtrees`: 5.0s on `rust-rustdesk-...-io-loop-medium-sized-file` (46.7k nodes).

Neither had ever shown up as a suspect because the metadata artifact drowned them: the old slowest-
fixture list was just "biggest files first." Fixing p99 now means profiling *inside* these two
passes (which pair, what shape, which APTED path), not re-architecting the pipeline's phase
structure - see the honest slowest-15 list in
`research/data/performance/benchmark_2026-08-17_warm_metadata.csv`.

### Roadmap to the goals, re-ranked against the honest baseline (2026-08-17)

Where things stand against each goal after the harness fix above (all numbers from
`research/data/performance/benchmark_2026-08-17_warm_metadata.csv`, 417 solved fixtures, idle machine):

- p50 < 100ms: **met**, 6.8ms (~15x headroom).
- p99 < 400ms: 1.13s, ~2.8x over; only 23/417 fixtures exceed 400ms, max 4.9s.
- 90% zero-mismatch: 310/417 (74.3%); need 66 more perfect fixtures to reach 376 (90%).
- Nonzero tail <= 0.5% of nodes: 37/107 nonzero fixtures currently violate the cap.

The 66 *easiest* nonzero fixtures each have <= 13 mismatches (327 total between them), so the 90%
bar does not require solving the big outliers - it requires converting near-misses. By cost class
(`algorithm_cost` vs `human_cost`) those 66 split: **48 search gaps** (`algorithm_cost >
human_cost`: a cheaper mapping exists and wasn't found), **12 cost ties** (cost function
under-discriminates - each one a cost-function bug report, see the correction in "Architecture
rethink" above), **6 cost-model-wrong** (`algorithm_cost < human_cost`: the model actively prefers
a non-human mapping). Corpus-wide, mismatches stay concentrated: top-10 fixtures hold 52% of all
2835, top-30 hold 82%.

**Runtime candidates, ranked** (the whole p99 problem is `solve_syntax_aware_matching`, per the
previous section):

1. **Budget the per-pair scoped APTED inside `solve_qualified_name_groups`.** First step is
   diagnosis, not code: temporarily log `(name, before_size, after_size, elapsed)` per
   `anchor_pair_via_apted`/`apted::for_nodes` call on the four known-slow fixtures and find which
   pair blows up and why (candidate suspects: one huge function body pair with flat children -
   Zhang-Shasha shape pathology, the known failure mode from the 2026-07-09 perf pass; or the
   group tie-break's `cost_ratio`/`sequence_edit_cost` estimate running O(pairs^2) inside a big
   N:M group). Then enforce a `before_size * after_size` budget **inside `resolve_forest`'s
   `Algorithm::Apted` branch itself** (the chokepoint principle from the rearchitecture plan: no
   caller, present or future, should be able to reintroduce an unbounded call), falling back to
   the bounded flat/leaf Myers path over that one pair. Expect quality interaction: these calls do
   real matching work, so A/B the full corpus and track the `qualified_name`-bucket fixtures
   individually, not just the total.
2. **Same treatment for `solve_large_flat_subtrees`** (5.0s on rustdesk at 46.7k nodes). Prior art
   to reuse, not rediscover: the 2026-08-16 "flat_children gate, revisited" entry (total-count
   gate + size cap), and the 2026-07-18 lesson "check the FLAT_MIN_CHILDREN fast-path first"
   before attributing cost to the main path.
3. **Make the O(n)-sweep passes residual-proportional** - matters for the giant-file tail
   (jellyfin, 258k nodes, 907ms with a trivial edit): `solve_leading_siblings` iterates *every*
   matched pair (~all nodes on a 99%-identical file) just to check `prev_sibling`; inverting it to
   iterate only unmatched comment/modifier nodes (via a maintained unmatched-set, or by walking
   `node_cache` filtered by kind) makes it proportional to the residual. Same shape applies to
   `solve_identical_diagnostic_statements`, the *second* `solve_bottom_up_propagation` sweep (seed
   a worklist from nodes newly matched since the first sweep instead of re-sweeping the whole
   tree), and `solve_unique_type_matching` (~60ms on jellyfin for a pass with zero corpus-wide
   firings - iterate only matched pairs that still have unmatched children, or flip its default
   off given the negative result above).
4. **Library-side hardening: make `metadata_of` cache-on-first-use** (e.g. `ast_metadata:
   OnceCell<ASTMetadata>` populated through `&Code`) so *any* future caller that clones a `Code`
   and diffs it stops silently paying ~26x metadata recomputes - the benchmark harness fix above
   only cures this binary. `Clone` should still reset the cell (node ids are parse-specific).
   Check the `Cow<'_, ASTMetadata>` return type's callers when converting - with a cell this can
   become a plain `&ASTMetadata`.

**Quality candidates, ranked** (target: +66 zero-mismatch fixtures):

1. **Branch/clause-level move detection for reordered if/elseif chains** - literature candidate #2
   above, design questions already scoped there (clause abstraction over Lua's bare-if vs.
   wrapped-elseif grammar quirk; guardrail against firing on non-swap order changes). Directly
   targets `lua-neovim-neovim-if-flips-two-branches` (68) and the commutative-reorder half of the
   `qualified_name` bucket (2026-08-17 survey above).
2. **A secondary tie-break objective for cost-tied mappings.** `javascript-microsoft-typescript-
   broken-js-remove-string-fragment` alone is 464 mismatches at an *exact* cost tie - the model
   cannot express whatever signal the human used. Most defensible candidate signal: locality/
   contiguity (prefer the mapping that minimizes crossings / preorder-index displacement between
   matched pairs, i.e. keep matches monotone and near their original neighborhood). Implementation
   hint: implement as a deterministic *tie-break* in candidate ordering (only consulted when unit
   costs are exactly equal), not as a new cost term - the 2026-07-11 container-dissimilarity
   revert showed how easily a new cost *term* breaks currently-perfect fixtures, while a pure
   tie-break provably cannot change any outcome that isn't already a tie.
3. **Per-fixture audit of the ~19 `algorithm_cost < human_cost` fixtures** (kotlin-remove-function
   -58, cpp-ladybird-refactor -53, cpp-laydbird-change-function-signature -48, kotlin-nextcloud
   -46, python-django -48, ...): each is a concrete proof the cost model prefers a wrong mapping.
   Diagnose individually with `--details`/`--dump` and classify *which* cost term is too cheap
   (likely `UnitCostModel::ren` on dissimilar containers - but the fix must be term-targeted and
   corpus-A/B'd, not a blanket similarity surcharge, which is exactly what was tried and reverted
   2026-07-11).
4. **LSH/MinHash candidate search for large rewrites** (literature candidate #3) - targets the
   `InsertWithChildren`-dominant outliers (`tsx-excalidraw` 231, `rust-zed-workspace-tasks` 117)
   where surviving content hides deep inside new structure. Highest effort, least evidence,
   explicitly deferred until 1-3 land.

**Sequencing note**: runtime #1 and quality #1/#3 touch the same code (`solve_qualified_name_
groups` and the scoped-APTED path), so start with runtime #1's diagnosis step - its per-pair logs
double as the evidence base for the quality work, and the budget it introduces must be in place
first so quality fixes aren't measured against a pipeline that later changes under them.

### Quality work: undecided nodes, and the wrap shape behind them (2026-08-17)

Started from the near-misses rather than the ranked list above, on the theory that the 90% bar is
reached by converting fixtures that are already close (the 66 easiest nonzero fixtures have <=13
mismatches each). Two false starts worth recording before the real finding:

- The 1-mismatch fixtures are dominated by a single leaf the human matched *across kinds*
  (`public` -> `protected`, `integer_value` -> `float_value`, `string_content` -> `regex`, `none` ->
  `identifier`), which `UnitCostModel::ren` forbids at cost 3 vs. delete+insert at 2. That looked
  like a systematic cost-model gap worth a new `kinds_update_allowed` family. **Checked before
  building: cross-kind is only 26 of 904 such mismatches corpus-wide**, and every one of the 13
  distinct kind pairs appears in exactly one fixture. It is a real gap but a long tail of
  one-offs, not a mechanism - the tiny fixtures just make it look representative. Not pursued.
- The near-miss reason breakdown pointed at `fast_fallback` and `qualified_name`, which is where
  everything ends up and says nothing actionable on its own.

**The real finding: codediff's mapping was not total.** Nodes present in neither `before_node_map`
nor `after_node_map` carried no decision at all - a consumer walking the after tree found nothing
for a genuinely new node and could not distinguish "inserted" from "never considered". 10 fixtures,
81 nodes. The cause is the ordinary shape of a *wrap*: in `typescript-add-error-handling` two
statements move inside a new `try`/`catch`, and the new `try_statement`, its `statement_block`, and
the `program` root above them all came out with no entry. The terminal Myers fallback only assigns
decisions to *maximal* unmatched roots, and neither a new wrapper (its subtree contains matched
descendants, so it is not maximal) nor an unmatched ancestor of matched children is one;
`solve_bottom_up_propagation` cannot help either, because its rule 3 requires all children to have
matched into the *same direct* after-parent and a wrap is exactly when they did not.

Three passes shipped, each measured independently against the full corpus, **none with a single
regression**:

| step | mismatches | zero-mismatch fixtures |
|---|---|---|
| after the runtime pass | 2835 | 310 |
| `solve_unresolved_nodes` (completeness sweep) | 2817 | 310 |
| + root pairing | 2807 | 311 |
| + `solve_mutual_ancestors` | **2785** | **312** |

1. **`solve_unresolved_nodes`** - terminal sweep giving every remaining undecided node the
   `Delete`/`Insert` its absence already implied. Pairs nothing, so it cannot guess wrong or take a
   partner from a later pass (there is none). `Delete`/`Insert`, never the `WithChildren` variants,
   since such a node usually *has* matched descendants. `algorithm_cost` rises 39235 -> 39380,
   which is the point: those nodes were always being dropped, they just were not being counted.
2. **Root pairing** (inside the same module) - the two trees' roots always correspond, so when a
   top-level wrap leaves both unclaimed they are paired directly rather than reported as the whole
   file being deleted and a new one inserted. Cost 0 / `MatchButNotIdentical`: `ren` already prices
   a same-kind internal pairing at 0, and running APTED over two whole files to rediscover that
   would cost more than the rest of the pipeline.
3. **`solve_mutual_ancestors`** - the general form of the same shape. For a before-node `B`, let
   `lca_after(B)` be the lowest common ancestor of every after-node that `B`'s matched descendants
   map to, and symmetrically `lca_before(A)`. Pair `B` and `A` only when **each is the other's
   LCA**. One direction alone would match a small container to a huge unrelated one that merely
   happens to contain the same content somewhere; requiring both means the two hold *the same*
   content and nothing else spoken for. Mutuality also makes the pairing unique by construction -
   two `B`s can share an `lca_after`, but only one can be that node's own `lca_before` - so there
   is no claiming order to get right, and no threshold or vote anywhere. Biggest single win:
   `html-mozilla-firefox-firefox-remove-li-around-button` 17 -> 2 (a `<li>` removed from deep
   inside had left every `element` ancestor above it, with *identical* paths on both sides, marked
   deleted). Gated by `HeuristicConfig::solver_mutual_ancestors`; ablation confirms 2785/312 with
   it on vs. 2807/311 off.

**Note for anyone extending this**: the rule needs **two or more matched descendants** in different
branches to say anything - with a single one, a container's LCA is that descendant itself and the
mutual test correctly fails. An early draft of the unit tests missed this and also went through the
full pipeline, where APTED had already claimed the containers before this pass ran; the tests now
pre-match statements and call `solve` directly (same isolation lesson as
`solve_unique_type_matching`'s tests, recorded above).

**Still open in this area**: ~38 of the original 81 undecided nodes remain mismatched, mostly
`element`/`content` chains in `xml-odoo-odoo-add-button-roles` (109) and the HTML fixtures, where
the human matched an ancestor that this rule still declines - typically because the container's
content is genuinely split across several after-parents, so no single LCA is mutual. Worth another
look before moving to the ranked quality candidates below.

### Quality, continued: four experiments, all negative, one strong lead (2026-08-17)

Follow-up to the section above, chasing the remaining wrap-family mismatches. Nothing here shipped
- recorded so the next session doesn't pay for it twice.

**Tried and reverted (no benefit):**
1. **Parent-correspondence tiebreak in `hash_tree_matching::solve_with_hash_map`.** Among
   equally-hashed candidates it prefers the one nearest by *byte offset*, which is the wrong signal
   when an edit shifts every later offset. Added a preceding key: prefer a candidate sitting under
   the after-parent this node's own parent was matched to (refining, not replacing, proximity).
   **Corpus effect: exactly zero - not one fixture changed.** Either the parent is rarely settled at
   that point, or the byte-nearest candidate already is the one under the right parent. Reverted
   rather than left as inert code in a hot path.
2. **LCS-anchored child alignment in `pair_children_for_descent`.** For non-commutative parents,
   children are paired by a naive positional `zip` + kind filter, so inserting one child shifts
   every later pair - which is *exactly* the shape of `xml-odoo-odoo-add-button-roles` (adding one
   `role="button"` attribute; the human maps `Attribute:2` -> `Attribute:3`, which a `zip`
   structurally cannot express). Replaced the zip with `myers_lcs` over `node_to_kind_and_value_
   hash` to anchor still-identical children, filling the gaps between anchors with the old
   positional zip. **Result: the target fixture did not move (still 109) and 5 previously-perfect
   YAML fixtures broke.** Reverted. Two lessons: the mispairing in that fixture is *not* happening
   in this function (worth finding out where before trying again), and monotone anchoring is not
   automatically safer than positional zipping - the YAML fixtures depend on the current behaviour.

**Ablation sweep of the two gated matching passes** (the 2026-07-15 study predates most of the
current pipeline, so it was worth redoing). Both are strongly net-positive - disabling
`solve_moved_subtrees` costs 2317 mismatches, disabling `solve_bottom_up_propagation` costs 311 -
so neither should be turned off. But the per-fixture breakdown is where the value is:

**`solve_moved_subtrees` actively harms 7 fixtures**, several severely:

| fixture | now | with move detection off |
|---|---|---|
| `cpp-laydbird-change-function-signature` | 60 | **8** |
| `python-django-...-update-unit-tests-actual-logic-change` | 48 | **0** |
| `kotlin-nextcloud-android-move-from-one-mocking-library-to-other` | 46 | 22 |
| `rust-rustdesk-...-io-loop-medium-sized-file` | 95 | 79 |
| `c-postgres-real-logic-change` | 27 | 14 |
| `java-genymobile-scrcpy-refactor-for-loop-in-a-function` | 52 | 46 |
| `c-nginx-add-typedef` | 50 | 44 |

That is ~180 mismatches of self-inflicted damage from a pass that is otherwise worth 2317. **This is
the strongest quality lead currently on the table**: the pass is right in general and wrong in a
characterisable minority, so a guard - not a threshold - is what's needed.

3. **`MIN_MOVE_SUBTREE_SIZE` sweep (4 -> 5/6/8/10/12), tried and reverted.** Diagnosis: a Python
   `string` is exactly 4 nodes (`string` + `string_start`/`string_content`/`string_end`), so at 4
   the pass pairs identical string literals in unrelated places as "moves" - all 48 of
   `python-django`'s mismatches. At 5 that fixture is **zero** and `rust-rustdesk` drops 95 -> 79.
   Against the project's real targets 5 even looks like a win (zero-mismatch fixtures 312 -> 313,
   fixtures breaching the 0.5% tail cap 33 -> 32, raw total 2785 -> 2803 - and raw total is not a
   stated goal). **Reverted anyway: it breaks 6 pinned `optimal_solutions` regression tests.**
   Buying one perfect fixture by loosening six regression guards is a bad trade, and a global size
   threshold is the wrong instrument regardless - it cannot help the three fixtures above
   (`cpp-laydbird`, `kotlin-nextcloud`, `c-postgres`) that only improve with the pass fully off,
   whose damage therefore comes from *large* false moves. Sweep table for the record: 4 -> 312
   fixtures/2785, 5 -> 313/2803, 6 -> 313/2923, 8 -> 312/3041, 10 -> 311/3128, 12 -> 308/3386.
   Past 6 it degrades on every metric, so 5 is a narrow optimum, not a plateau.

**Recommended next step**: characterise *why* move detection fires wrongly on `cpp-laydbird`
(60 -> 8) and `python-django`, then add a targeted guard. The size threshold and the existing
container-identity agreement rule are both too blunt. Concrete hypothesis worth testing first, from
the `python-django` diagnosis: require a move candidate to contain real nested structure (at least
one non-leaf child), which excludes a bare literal-with-delimiters regardless of node count, and
unlike a size bump does not penalise legitimately small structured moves - the 11 fixtures that
regressed at threshold 5 suggest those exist and matter.

### `is_commutative_container` cannot fix reordered siblings - wrong layer (2026-08-18)

`css-wordpress-reformat` (30 mismatches, 15.6% of its nodes - the corpus's second-worst by
percentage) looked like the perfect customer for the long-standing "commutative sibling matching"
idea. It isn't, and neither is anything else: **adding container kinds to
`nodes::is_commutative_container` is measurably incapable of fixing this class of fixture.** Tried,
measured, reverted; do not retry without first reading the mechanism below.

**What the fixture actually is** (the name misleads - it is not merely a reformat): a minified
WordPress CSS file pretty-printed, *and* `margin-top`/`margin-bottom` swapped inside all four rules.
The human maps each declaration across the swap (`declaration:3` <-> `declaration:2`); codediff pairs
them positionally and must then explain the swap as an `Update` of `property_name` plus updates to
every node beneath, so four swaps cascade into 30 mismatched nodes.

**Node kinds, verified against the real grammars** (2026-08-18, by dumping parse trees - never
guessed, given this function's own history of dead strings): CSS `rule_set > block > declaration*`;
XML `STag`/`EmptyElemTag > Attribute*`; HTML `start_tag`/`self_closing_tag > attribute*`; Java
`modifiers > public|static|final`. XML and HTML attributes are order-independent *by
specification*, Java modifier order is free per the JLS, so all four are legitimate entries on the
merits.

**All four measured at exactly zero corpus effect** - not one fixture changed, individually (CSS
alone) or together. The reason is structural, and it is the useful part of this entry:

- `is_commutative_container` only influences two things: order-independent *hashing*
  (`code::hash`), and child pairing inside `pair_children_for_descent` during *hash descent*.
- These fixtures never reach either. `--dump` shows `css-wordpress-reformat`'s block pair resolved
  by `APTED("fast_fallback")`. The blocks cannot hash-match even with order-independent hashing,
  because the minified file's final declaration in each rule has no trailing `;` while the
  pretty-printed one does - one genuinely different child is enough to change the whole multiset
  hash. So the pair falls through to tree edit distance.
- **Ordered tree edit distance structurally cannot express a sibling swap.** It is order-preserving
  by construction, so the human's crossing mapping is not merely unfound, it is unreachable in that
  layer. APTED never consults `is_commutative_container` and could not act on it if it did.

**What would actually fix it**, for whoever picks this up: a pre-match that pairs hash-identical
children of a commutative container *before* the DP sees them, so the crossing is already committed
and the DP only resolves what is left. The awkward part is anchoring - the block pair here is only
known *after* the fallback matches it, so a pre-pass has nothing to hang off. The viable shape is
probably a post-fallback repair restricted to provably cost-reducing swaps (a child currently
carrying a nonzero-cost `Update` when a zero-cost hash-identical partner exists under the same
matched container), which strictly lowers total cost and so cannot be a regression under the cost
model - unlike a general re-pairing, which would be stealing from an existing mapping and is
something every other pass here is careful never to do.

This likely also covers `xml-odoo-odoo-add-button-roles` (109) - same shape, an inserted
`role="button"` attribute shifting the rest, where the human maps `Attribute:2` -> `Attribute:3` -
and the HTML fixtures. Worth doing; worth doing properly.

### Crossed-sibling repair: works, but net-negative as built (2026-08-18)

Built the post-fallback swap repair the previous entry proposed, as `solve_sibling_swaps` (reverted;
rebuild from this entry rather than from memory). Shape: group the *nonzero-cost* mappings by their
matched parent pair - so the pass is proportional to what actually changed, not to the file - and
inside a group look for two pairs `b1<->a1`, `b2<->a2` where re-pairing crosswise is cheaper.
Latency was a non-issue at every stage (p50 3.8ms unchanged, p99 764 -> 777ms, total 19.2 -> 19.5s),
helped by a `MAX_SWAP_SUBTREE` cap and by only ever re-pointing subtrees that are already suspect.

**It does what it was built to do.** `css-wordpress-reformat` **30 -> 2**,
`rust-add-comments-and-real-new-logic` 84 -> 61, `tsx-mui-material-ui-move-colour-to-a-new-attribute`
16 -> 7.

**And it is still net-negative**: five previously-*perfect* `yaml-draios-sysdig-*-url-change`
fixtures went 0 -> 8..24, so zero-mismatch fixtures fell 313 -> 308 and total mismatches rose +12.
Not shipped.

Two things were tried and did *not* explain the YAML damage, so don't repeat them:
- Requiring **both** crossings to be byte-identical. Provably safe, but too strict to fire on the
  real target: in `css-wordpress-reformat` the minified side's last declaration per rule has no
  trailing `;`, so only one direction of the swap is exact even though the swap is plainly right.
  This is why the decision was relaxed to a cost comparison in the first place.
- Excluding `is_commutative_container` parents (on the theory that hash descent already handles
  those correctly). **Zero effect** - byte-identical corpus numbers. The YAML damage is not under a
  commutative parent at all: `--details` puts it inside a `flow_sequence`, YAML's inline `[a, b]`
  list, which is ordered.

~~**The real defect is the cost estimator, and that is the useful finding.**~~ **Retracted
2026-08-18 - this diagnosis was wrong, and the correction below is the useful finding.** The claim
was that `sequence_edit_cost` (blind to a child that merely *resembles* its counterpart) made the
crossing look cheap, so a similarity-aware estimator would fix the YAML damage. It would not. A
similarity measure was built (`code::similarity`) and it reads the YAML fixture *perfectly* - each
`flow_node` scores 1.00 against its true permuted counterpart and 0.33 against every other, so the
permutation is unambiguous. **That is the wrong answer**, and a better estimator only reaches it
faster:

```
yaml-draios-sysdig-string-url-change, human_mapping.json
  match_but_not_identical | flow_node:1 -> flow_node:1
  ... all six, positional, though every one is byte-identical to a flow_node elsewhere
```

**The ground truth is positional. The objective was wrong, not the estimator.** The benchmark's own
cost columns then looked like they explained *why*, and that reading was itself a bug - the numbers
below are kept only because the argument built on them is referred to later:

| fixture | algorithm_cost | human_cost | |
|---|---|---|---|
| `yaml-draios-sysdig-string-url-change` | ~~0~~ **6** | ~~0~~ **6** | corrected by the cost fix, same day |
| `css-wordpress-reformat` | 8 | 4 | unchanged |

The original reasoning ran: the differing URL text lives in *gap text* between the two quote leaves,
which no cost path charged for, so both mappings cost 0, so chasing the permutation buys nothing -
which is why the human left it positional. **Two things in that are wrong.** The zero came from
`cost::operation_cost` pricing `MatchButNotIdentical` at 0, *not* from `UnitCostModel::ren` (fixing
`ren` alone left these columns untouched - the two are separate paths, DP search vs. reported
score). And with the zero fixed the fixture reads 6/6, so the permutation - every pair
byte-identical, `COST_MOVE` 0 - is *cheaper* than the positional ground truth, not equal to it. The
human chose the dearer mapping. In CSS the crossing genuinely halves the cost, 8 -> 4, and the human
traces it (`declaration:3 -> declaration:2` alongside `declaration:2 -> declaration:3`) - so the two
fixtures disagree about whether ground truth minimises cost at all. See the gap-text section below.

~~**So the discriminator a rebuilt swap pass needs is "does crossing strictly reduce *real* cost".**~~
**Also retracted, same day, by the cost fix in the gap-text section below** - once gap-owned text is
charged, this fixture's positional ground truth costs 6 and the permutation costs 0, so the human
chose the *dearer* mapping and no cost rule separates it from `css-wordpress-reformat`, where they
chose the cheaper one. The `0 / 0` reading that suggested "cost-neutral, so the human broke the tie
on simplicity" was itself a symptom of the bug. What remains true is that `sequence_edit_cost` is a
*different* cost function from the one the benchmark scores against, and that they disagree
precisely where content sits in gap text. Note
also that such a pass must not retract an existing match: every pass in this pipeline is monotone
(matching only ever adds), which is what makes residual filtering behaviour-preserving, and
`ASTDiff` deliberately offers no match-removal API. The decision has to be made where the pairing
is *first* made - inside the aligner - not repaired afterwards.

### The similarity sketch (2026-08-18, shipped)

The idea recorded here as speculation - "give each node a similarity signature alongside its
equality hashes" - was built, measured, and kept. `code::similarity::SimilaritySketch` is a
bottom-k MinHash over the set of content-token hashes in a subtree, computed in the same post-order
walk as the four existing hashes: O(n) at metadata time, O(k) to compare two arbitrary nodes
without walking either subtree. It is *exact*, not estimated, whenever both subtrees have <= 16
distinct tokens, which covers most nodes in a real file, and `jaccard` divides by the union size
rather than by k so small subtrees are not systematically under-reported.

**Two design points that measurement decided, not taste.**

*Sketch leaves, not every descendant.* All-descendants sounds more discriminative and is strictly
worse for the near-identical case this exists for: one changed token flips the full hash of every
ancestor inside the subtree, so a one-token edit costs O(depth) set elements instead of 1.

*A node's own gap text counts as a token.* Found the hard way. tree-sitter-yaml keeps a quoted
scalar's body in the *gap* between its two quote leaves, so a leaf-only sketch made six completely
different URLs in a `flow_sequence` identical - every pairing scored 1.00. Grammars disagree about
whether a scalar's body is a child node or gap text, and a similarity measure must not depend on
which choice a grammar made. `hash::compute_owned_text_hash` pulls the gaps out for this; note the
same blindness is worth checking for in any *other* consumer that reasons about leaves.

**What it bought, and what it did not.**

Its intended customer - the crossed-sibling repair - turned out not to need it, and the entry above
records why at length: that fixture's ground truth is positional and cost-neutral, so no similarity
measure however good would have prevented the regression. **A negative result that cost one
afternoon instead of a rebuilt pass, because the sketch was aimed at the failing fixture before any
consumer was written.** Do that first next time too.

The customer it *did* find was `solve_moved_subtrees`' ambiguity guard. That guard refuses to pick
between several identical small move targets, on the sound reasoning that the candidates are
indistinguishable - they share a full hash, which is why they are candidates. But their
*surroundings* need not be indistinguishable, and comparing two containers is exactly the O(k)
question the sketch answers. `disambiguate_by_context` compares the parents and accepts the winner
only when it leads by `CONTEXT_TIEBREAK_MARGIN`:

| | mismatches | zero-mismatch | > 0.5% tail | p50 | p99 | total |
|---|---|---|---|---|---|---|
| baseline | 2747 | 313 | 33 | 3.70ms | 802ms | 19.0s |
| **shipped** | **2725** | 313 | 33 | 3.73ms | 801ms | 19.1s |

-22 mismatches, no regressions, and latency unmoved *within single-run noise* - `elapsed_ms` is one
unrepeated measurement per fixture, and a no-op build of this same primitive drifted +1.6% on the
`total` column, so read those three columns as "no visible cost", not as a tight measurement. The
mismatch counts are not noisy: verified 20x in separate processes (76/231/19 every time), the
separate-process form this project's `benchmark-determinism-fix` requires, since a new tiebreak is
exactly where parse-unstable ordering has bitten before. `html-apache-echarts` 90 -> 76, `tsx-excalidraw`
235 -> 231, `vimscript-...-hex-colours` 23 -> 19 (below the 22 it sat at *before* the ambiguity
guard). **`algorithm_cost` falls in all three** (647->619, 1040->1032, 5159->4653), so these are
better pairings by the project's own objective, not just closer to the labels - the check worth
making before believing any mismatch-count improvement.

`MAX_AMBIGUOUS_CANDIDATES` is a pure cost bound, not a quality/cost tradeoff. Uncapped, scoring
every candidate of a commodity hash (a `,`, a `;`, `self` - hundreds of them) made the whole corpus
~8% slower; at 8 it cost 4 mismatches; **at 32 the quality is identical to uncapped**, i.e. no
fixture in the corpus needed more than 32 candidates, so 32 is simply the smallest cap tried that
loses nothing.

**Cost of the primitive itself**: ~1.5% of whole-corpus metadata time (105.9s vs 104.4s total, two
runs each), and nothing at diff time. Note `benchmark_optimal_solutions` warms metadata *outside*
its timer, so `elapsed_ms` cannot see metadata-time work at all - a change like this one has to be
measured on total wall time or it will look free when it is not.

**Consumers not yet tried**, in rough order of promise: `qualified_name`'s group tie-break; a
pre-filter for candidate search in large rewrites (`rust-zed-workspace-tasks` and friends, the
"search-quality gap" bucket); and ranking inside `solve_large_flat_subtrees`. Two standing caveats.
The 2026-07-11 container-dissimilarity revert was a *cost-model* change (a text-similarity surcharge
on `ren`) and broke five perfect fixtures - a sketch used for *candidate selection* is a different
use and should not be assumed to inherit that failure, but must be validated separately. And it is
an estimate above 16 tokens: rank and gate with it, never conclude equality, which the exact hashes
already answer definitively.

### Gap-owned text: what tree-sitter puts *between* the children (2026-08-18)

Prompted by an obvious follow-up question after the sketch's YAML surprise - if a node's content can
live in gap text rather than in a child, is that information *lost*? **No, and establishing that is
the useful part.** Every node's full span is already in `ASTNodeMetadata::text`, and
`compute_full_hash` has always hashed the gaps, so equality matching was never blind. What is blind
is any logic that *enumerates leaves*, and (as measured below) `compute_structural_hash`, which
ignores gaps by design.

**Corpus census** (`code::gap_survey`, an `#[ignore]`d diagnostic - re-run it if a new language is
added). Internal nodes owning non-whitespace text, both sides of all 418 fixtures:

| language | such nodes | biggest kinds |
|---|---|---|
| XML | 21665 | `AttValue` 21663 nodes / 394KB - i.e. **every attribute value** |
| CSS | 7128 | `integer_value` 5517, `color_value` 1445 |
| Rust | 2288 | `line_comment` 2103 / 126KB, `block_comment` 46 / 20KB |
| YAML | 1878 | `double_quote_scalar` 1095, `single_quote_scalar` 749 |
| Vimscript | 1723 | `hl_attribute` 384, `list` 284 |
| Scala | 420 | `interpolated_string` 294 |

Zero in TypeScript, JSON, Go, Kotlin, JavaScript, C++, TSX, Java - which is why this had never
surfaced.

**Fixed: `UnitCostModel::ren` now charges for it.** `ren` returned 0 for same-kind internal nodes
because "children cost is accounted for separately", which is exactly false for these - relabelling
`role="button"` to `role="menu"` cost nothing, and matching an `AttValue` to a completely unrelated
one was equally free, leaving the DP no reason to prefer the right partner. `ASTNodeMetadata` gained
an `owned_text_hash` (hashed, not stored as text: the consumer is an equality test in the per-DP-cell
hot path) and a difference is priced as `COST_UPDATE`.

**`COST_UPDATE`, not `COST_LITERAL_UPDATE`, and the difference is the whole result.**
`COST_LITERAL_UPDATE` is 2 - *exactly* `COST_DELETE + COST_INSERT` - so pricing the relabel there
leaves the DP indifferent between "this attribute's value changed" and "this attribute went and an
unrelated one arrived". That is not theoretical: at 2 the fix was -1 mismatch *with* a regression
(`css-wordpress-...-change-simple-values-to-vars` 1 -> 2, breaking its pinned ceiling); at 1 it is
**-2 with no regression at all** (`css-mozilla-firefox-actual-style-changes` 8 -> 6, and nothing
else in the corpus moves). A relabel must be strictly cheaper than delete+insert - the same premise
the different-kind branch states when it deliberately goes one *above* that sum to forbid a pairing.

Corpus effect is small - **-2 mismatches** (2725 -> 2723), one fixture moved - and latency is
unchanged or slightly better: over three runs each, `sum` 18904/18945/18973 with the fix against
19135/19140/20831 without. **A first single-run measurement said "+4% and p99 801 -> 935ms" and that was pure noise**;
it was nearly the reason this got reverted rather than shipped. `elapsed_ms` is one unrepeated
sample per fixture - do not act on a single-run latency delta under ~2x, which this file has said
before and which is easy to forget when the number happens to confirm a decision you were already
leaning towards.

It ships on correctness, not on the metric: the model was reporting a cost it knew to be wrong.

**Why the corpus barely moved, which is the other finding worth keeping.** On the fixture that
motivated it, `xml-odoo-odoo-add-button-roles` (109 mismatches, 81 involving `AttValue`), **not one
mismatch is decided by `ren`**:

| reason | count |
|---|---|
| `StructurallyIdenticalAncestor` | 41 |
| `APTED("fast_fallback")` | 23 |
| `UnresolvedNode` | 22 |
| `MovedSubtree` | 16 |

The cost model was never a party to these decisions. **Check which pass owns a fixture's mismatches
before improving a pass** - `--details <fixture> | grep -o 'reason ...'` is the whole check. It does
not make the cost fix wrong, but it does predict, in seconds and before any code, that the fix
cannot move *this* fixture.

**The same hole on the scored path, now also fixed.** `cost::operation_cost` priced
`MatchButNotIdentical` at 0 unconditionally, on the same premise ("the differences below are charged
on the descendant entries") and with the same exception: when the difference is in gap text there
*are* no descendant entries carrying it. It now charges `COST_UPDATE` when the paired nodes'
`owned_text_hash` differ, on both sides of the comparison (`diff_cost` and
`human_mapping_cost`, which must stay under one cost model or the two columns stop being
comparable). `diff_cost` feeds only the benchmark's reporting, so **no mismatch count moved** - 24
fixtures' cost columns did, and `yaml-draios-sysdig-string-url-change` went from `0 / 0` to
**`6 / 6`**, i.e. exactly the six changed URLs it had been reporting as free. Sanity checks: the
count of fixtures where `algorithm_cost < human_cost` is unchanged at 14, and where the algorithm
reproduces the human's mapping both columns move together (`+6 / +6`), while where they differ only
the algorithm pays (`xml-odoo` `+4 / +0`, widening 115/30 to 119/30 - the gap it should have been
showing all along).

**This falsifies the "use real cost as the discriminator" recommendation made above.** With costs
now honest, the yaml fixture's *positional* ground truth costs **6** while the permutation the
sketch identifies costs **0** (every pair byte-identical, and `COST_MOVE` is 0). So the human chose
the strictly more expensive mapping - the earlier reading, "both cost 0, so the human broke the tie
on simplicity", was an artefact of the very bug this section fixes. A cost-driven crossed-sibling
repair would therefore *still* break these five fixtures.

Where that leaves the crossed-sibling problem: the human traces the crossing in
`css-wordpress-reformat` (cheaper: 4 vs 8) and refuses it in `yaml-draios-sysdig` (dearer: 6 vs 0),
so **ground truth here is not cost-minimising and no cost rule can separate the two**. The remaining
candidate distinction is semantic rather than numeric - a CSS `declaration` is a named entity
(`margin-top` is a thing with an identity that can move), a YAML list row is anonymous data whose
position *is* its identity - which is testable against the corpus but has not been tested. Do not
build the pass on a cost rule.

**The lead it did produce.** `StructurallyIdenticalAncestor` is the single biggest contributor, and
it rests on `compute_structural_hash`, which hashes *only* kinds and child counts - gaps are
deliberately excluded. On a gap-heavy language that is a much stronger claim than intended: two XML
`AttValue`s with completely different values are "structurally identical", and so is every ancestor
above them, so the pass pairs their children positionally on evidence that excludes the only bytes
that actually differ. Note this is not automatically wrong - `yaml-draios-sysdig`'s positional
pairing is exactly what its ground truth wants - so the fix is not "hash the gaps into the
structural hash" but "find out where positional-under-a-structural-match is and isn't right", with
XML's 41 as the worked example.

### Systematic scan for the two 2026-08-18 bug classes (2026-08-18)

After the `ren`/`operation_cost` fixes, a full sweep of the codebase for their two underlying
classes: (a) **"children carry the cost/content"** stated or assumed somewhere gap-owned text
falsifies it, and (b) **cost ties** - a relabel priced *exactly* `COST_DELETE + COST_INSERT`,
which is neither a preference nor a prohibition but a coin flip.

**Found and fixed (class a, consistency - output-neutral, verified 0 fixtures changed either
mismatches or cost columns):**

- `apted::classify_match`: contract is "cost of relabeling just the root pair", but the
  same-kind-not-identical branch recorded 0 even when `owned_text_hash` differs - so
  `mapping.cost` disagreed with what `ren` had just paid in the search *and* with what
  `operation_cost` scores. `mapping.cost` drives no decisions (verified: no comparisons on it
  anywhere), but twice today a recorded zero that "didn't matter" derailed a diagnosis.
- `hash_tree_matching::classify`: same hole in the descent classifier.
- ~~`test::helper::optimal_iud`: same hole in the oracle tests compare against.~~ Moot: that
  module was deleted 2026-09-03. No test ever compared against it - its only caller was a bench
  that neither compiled nor had a Makefile target, so the under-pricing bug was in an oracle
  nothing ran.

**Found and fixed (class b, behavioral):** `COST_LITERAL_UPDATE` was 2 = `COST_DELETE +
COST_INSERT` exactly, documented in `rust_sniffnet_protocol.rs` as an "accepted cost tie" that
APTED resolves as delete+insert against the human's obvious Update. Swept both escapes: **raising
to 3 changed nothing corpus-wide** - i.e. the tie already always resolved toward delete+insert, so
"2 to discourage" had silently been a forbid all along - and **lowering to 1 is -4 mismatches,
+1 zero-mismatch fixture** (2723 -> 2719, 313 -> 314, tail unchanged at 33): `rust-sniffnet-protocol`
1 -> 0 (test un-clamped), `vimscript-...-add-two-functions` 22 -> 18 (ceiling lowered), and one
deliberate +1 on `rust-add-comments-and-real-new-logic` 84 -> 85 whose `algorithm_cost`
*improved* 273 -> 271 (ceiling raised with rationale in the test). The rule now written on the
constant: below `COST_DELETE + COST_INSERT` is a preference, above it is a prohibition, and
nothing sits on the boundary. Latency: single-run sums skewed high (20.6-23.1s vs the 18.9-20.8s
band of earlier same-day runs) but within this box's demonstrated same-build variance; mismatch
results are deterministic (3/3 identical runs).

**Scanned and sound (so the next sweep doesn't redo it):**

- `nodes::map_identical_descendants`' positional zip - both callers (`solve_leading_siblings`,
  `solve_identical_diagnostic_statements`) precondition on byte-equality (full text / full hash).
- `compute_kind_and_value_hash` - hashes gap text at every level, like `compute_full_hash`.
- `hash_tree_matching`'s reorder patch-up - downgrades `Identical` ancestors and charges
  `COST_UPDATE` at the reordered parent.
- `hash_tree_matching::classify`'s `Identical` arm - keys on the kind+value hash, which includes
  gaps, so gap-differing pairs can't classify as `Identical`.
- `solve_moved_subtrees` (full-hash pairing), `solve_greedy_anchor_blocks`' `sequence_edit_cost`
  (full hash), `solve_unresolved_nodes` root pairing (scored via `operation_cost`, which now
  checks owned text).
- `UnitCostModel::del`/`ins`, `subtree_del_cost`/`subtree_ins_cost` - per-node constants; a
  deleted gap-owning node's text goes down with the node, correctly.
- `FORBIDDEN_RENAME_COST` = del+ins+1 - correctly *above* the boundary, per the tie rule.
- `COST_MOVE` = 0 on both the diff and human sides - consistent, a deliberate design choice, not
  a tie (nothing competes with Move at 0).
- `compute_structural_hash` excludes gaps *by design* - it answers "same shape?"; the open
  question about `StructurallyIdenticalAncestor` positional pairing on gap-heavy languages is
  tracked in the gap-text section above, as a matching-policy question, not a hash bug.

### Move detection: the ambiguity guard (2026-08-17, shipped)

Followed the lead above. `--dump` on `python-django` showed all 48 mismatches came from three
shapes paired across unrelated classes: a bare `string` (`string_start`/`string_content`/
`string_end`), an `attribute` (`identifier`/`.`/`identifier`), and a `parameters` list
(`(`/`identifier`/`)`) - each exactly four nodes.

**The structural hypothesis recorded above was tested and is wrong.** "Require a non-leaf child"
does exclude all three shapes, and does take `python-django` to zero - but it regressed **12**
fixtures, because in CSS, JSON and vimscript colour tables the content that legitimately moves *is*
flat by nature. Structure is not the discriminator; net +24 mismatches, same wash as the threshold
bump. Don't retry it.

**What actually separates them is ambiguity, not shape.** A `self.foo` occurs dozens of times per
file, so when several available targets spell the same thing, "which one did this move to" has no
answer and the document-position tiebreak is a guess dressed as a decision. Refusing to pair in
that case fixes `python-django` outright. Unbounded, though, it costs the data-heavy files real
matches (their repeated rows genuinely do move), so it is gated by `AMBIGUOUS_MOVE_MIN_SIZE`:
ambiguity is only disqualifying below ~8 nodes, above which a repeated subtree is distinctive
enough to trust. Swept:

| variant | mismatches | zero-mismatch fixtures |
|---|---|---|
| baseline | 2785 | 312 |
| structure guard (non-leaf child) | 2809 | 313 |
| ambiguity, no size gate | 2790 | 313 |
| ambiguity below 6 | 2753 | 313 |
| **ambiguity below 8 (shipped)** | **2747** | **313** |

Shipped at 8: **-38 mismatches, `python-django` 48 -> 0, `rust-rustdesk` 95 -> 79,
`java-genymobile-scrcpy` 52 -> 46**, fixtures over the 0.5% tail cap unchanged at 33. Note this is
a *different question* from `MIN_MOVE_SUBTREE_SIZE`, which gates whether a subtree is worth
considering at all; this gates whether a choice *between several equals* can be made honestly.

**Honest cost, not hidden**: four fixtures regress - `vimscript-...-hex-colours` 11 -> 23,
`vimscript-...-debian-package-parsing` 8 -> 16, `html-apache-echarts` 82 -> 90, `tsx-excalidraw`
231 -> 235. All are repeated-content files where the position tiebreak had been guessing correctly.
The first pushed a pinned ceiling over its limit; it was raised 22 -> 24 **deliberately**, with the
reasoning written into that test, rather than silently. Net effect is positive on every target
metric, which is why it shipped, but a future session looking at those four fixtures should know
this guard is why.

**Still unfixed**: `cpp-laydbird-change-function-signature` (60, would be 8 without move detection),
`kotlin-nextcloud-...-mocking-library` (46 -> 22) and `c-postgres-real-logic-change` (27 -> 14) are
untouched by this - their damage comes from *large* false moves, where neither size nor ambiguity
applies. That is a separate mechanism and the next thing to characterise here.

### Runtime work done: the APTED budget was the wrong fix; the constant factor was (2026-08-17)

Runtime candidate #1 above proposed budgeting `solve_qualified_name_groups`' scoped APTED calls.
Its mandatory diagnosis step killed the plan and replaced it with a strictly better one - **no
budget was implemented, and none should be until the numbers below stop improving.**

**What the diagnosis found.** Per-pair instrumentation (temporary, fully reverted) at the
`resolve_forest` chokepoint, logging each call's *pruned residual* sizes (`PostorderIndexer::size`,
i.e. what survives after already-matched subtrees are pruned - not raw subtree size) against
elapsed time:

- Raw subtree size is a *useless* predictor: a pair with `product = 26.9M` finished in 5.5ms while
  one with `product = 391k` took 1154ms. Pruned residual predicts far better but still only
  loosely. This independently re-derives the 2026-07-25 "size/dissimilarity-capped fallback"
  revert - **any budget keyed on size would fire mostly on the wrong calls**, which is exactly why
  the planned budget was not built.
- Corpus-wide, 131.5s of APTED time, 87% of it under `qualified_name`, concentrated in 118 calls.
- The damning number: **~2.7µs per unit of residual product**, i.e. per DP cell - a hundred times
  what a tree-edit-distance inner loop should cost. That pointed at the constant factor, not the
  algorithm or its inputs.

**Root cause.** `UnitCostModel::ren` is evaluated once per DP cell (O(n1*n2) and more). It called
`kinds_update_allowed`, which linearly scanned `IDENTIFIER_KINDS` twice plus up to six operator-
family arrays comparing `&str`s - **tens of string comparisons per cell**. Every input to those
scans depends only on a node's *kind*, never on the pair being compared. A stub experiment
(replacing `ren` and the containment adjustment with constants) bounded the recoverable share at
~50% of all APTED time before any real work started.

**Fix 1: `code::KindCostClass`, precomputed per node** (`identifier_like`, `literal_like`,
`operator_families: u8`), plus `UnitCostModel::language_family_mask: u8` computed once per cost
model. The cross-kind test becomes `(a.operator_families & b.operator_families & language_mask) !=
0` - exactly equivalent to `families.any(|f| f.contains(a) && f.contains(b))` restricted to the
language's families, with no assumption that a kind belongs to at most one family (several do:
`+` is in both `ARITHMETIC_OPS` and `PHP_ARITHMETIC_OPS`). The masks are *derived from the same
`const` arrays* `kinds_update_allowed` still uses (`ALL_OPERATOR_FAMILIES` +
`families_for_language`, the latter extracted from that function's own `match`), so the fast and
slow forms cannot drift; `operator_family_masks_agree_with_string_scanning_kinds_update_allowed`
pins that exhaustively over every known kind pair x every language. `UnitCostModel::new` is now the
only constructor, so `language_family_mask` can't fall out of step - and `language` itself turned
out to be dead afterwards and was removed.

Measured on the APTED calls themselves: rustdesk 4188ms -> 1492ms (2.8x), kotlin-nextcloud 1758 ->
906 (1.9x), excalidraw-ts 1260 -> 790 (1.6x). Corpus effect of this fix alone: p99 1132ms -> 990ms,
max 4886 -> 3582, fixtures over 400ms 23 -> 17, total corpus diff time 38.8s -> 32.3s. After this,
stubbing `ren` entirely changes nothing measurable - it is off the critical path.

**Fix 2: three whole-file sweeps made residual-proportional.** Each cost O(file) even when the edit
was one token, which is what made a 258k-node fixture with a one-string-literal change take ~900ms:
- `solve_leading_siblings` iterated *every* matched pair (~every node on a near-identical file),
  paying two `node_cache` lookups, two tree-sitter `prev_sibling` calls and two `node_map` probes
  each, just to discover there was no leading comment to match (~400ms). Now pre-computes the set
  of anchors whose immediately-preceding sibling is an unmatched comment/modifier - the necessary
  condition for the walk to do anything - in one O(n) pass, and skips everything else.
- `solve_bottom_up_propagation` collected and *sorted* every node in the file (~260ms across its
  two calls); now filters to unmatched non-leaf nodes before sorting.
- `solve_unique_type_matching` sorted every matched pair (~60ms) for a pass that fires zero times
  corpus-wide; now filters to pairs that actually have an unmatched child.

All three filters are behaviour-preserving for the same structural reason, worth stating once
because it is what makes them safe: **matching only ever adds entries to the node maps, never
removes them**, and `children` is immutable, so a node excluded by these filters could not have
become eligible later in the same pass - while a node that becomes matched *during* a pass is still
caught by the in-loop guards, which were all left in place.

**Combined result, full corpus (417 fixtures, idle machine,
`research/data/performance/benchmark_2026-08-17_after_runtime_pass.csv`)**, against the honest baseline in
the previous section:

| | p50 | p90 | p99 | max | >400ms | total | mismatches | zero-mismatch |
|---|---|---|---|---|---|---|---|---|
| baseline | 6.8ms | 172ms | 1132ms | 4886ms | 23 | 38.8s | 2835 | 310 |
| + fix 1 | 6.6ms | 125ms | 990ms | 3582ms | 17 | 32.3s | 2835 | 310 |
| + fix 2 | **3.8ms** | **81ms** | **833ms** | **3086ms** | **11** | **20.7s** | 2835 | 310 |

**Zero fixtures changed mismatch count at any step** - both fixes are pure constant-factor work,
verified fixture-by-fixture, not just on the total. Roughly 1.9x end-to-end (38.8s -> 20.7s), p90
2.1x, and the count of fixtures breaching the 400ms target is down from 23 to 11. p50 (3.8ms)
clears its <100ms goal by ~26x; **p99 833ms remains ~2x over the 400ms goal**, so this pass narrows
but does not close the latency gap.

Caveat on precision, since these are the numbers the next session will measure against:
`elapsed_ms` is a single unrepeated timing per fixture, and repeated runs of the *same* binary
moved the tail figures by ~5% (an earlier run of the identical code read p99 787ms / max 2626ms /
8 over-budget). Treat sub-10% deltas in p99, max and the over-400ms count as noise; the totals and
the p50/p90 columns are far steadier, and the fixture-by-fixture mismatch comparison is exact.

**What did NOT work, measured and reverted**: an `is_trivial()` early-out on `ContainmentCtx::
adjust` (skip the hash probes when nothing is pruned and there are no sibling-order anchors).
Zero measured effect - real contexts on the hot fixtures have non-empty pruned-target maps, so the
fast path never fired. Removed rather than kept as speculative code.

**Where the remaining APTED time is - measured, and it is NOT a constant factor.** Three follow-up
experiments (all instrumented, measured, and fully reverted; nothing below is shipped code):

1. **Subproblem count.** Counting `vren` calls per `resolve_forest` call: the two hottest pairs run
   **24.8M and 12.5M** cell evaluations for residual products of 629k and 461k - i.e. **12-40x
   `n1*n2`**, confirming the suspicion recorded above. This is APTED doing its normal
   super-quadratic path-decomposition work, not a bug.
2. **`ContainmentCtx::adjust` is a dead end, contradicting the ~15% estimate above.** With the
   cell counter in place, stubbing `adjust` entirely moved per-cell cost only 64ns -> 59ns
   (rustdesk) and 145 -> 138 (kotlin) - **~5%, not 15%**. The earlier 15% figure was measured
   before fix 1 landed and did not survive it. The planned "index containment by dense preorder"
   rewrite is therefore **not worth doing** - it is real work for at most a 5% return.
3. **Per-`spf_a` allocation is not the problem either.** `spf_a` allocates two matrices plus three
   whole-tree-sized vectors on every call, which looked like a classic O(n^3)-allocation pitfall.
   Counted: 2053 calls and 6.3M allocated elements against 24.8M cells - **0.3-0.8 elements per
   cell**. Ruled out. Do not re-litigate this by reading the code; the counter already answered it.

**So the last ~2x on p99 cannot come from micro-optimization** - at ~60-150ns per cell with tens of
millions of cells, the only lever left is handing APTED *less work*, which means changing what gets
matched and therefore has a quality cost. Two candidate shapes, neither implemented, both needing a
full corpus A/B before being trusted:
- **Decompose an over-budget pair into anchor-separated segments** and resolve each independently
  (the `split_into_anchored_segments` machinery `resolve_residual_forest_via_myers_lcs` already
  uses). Cost is the sum of several small super-quadratics instead of one large one. **Known
  catch, and the reason this is not a free win:** that splitter only recurses into real APTED for
  *equal-count* segments; unequal-count segments fall through to atomic delete/insert. On a large
  residual - which is exactly when the budget would trip - segments are likely to be
  unequal-count, so this degrades toward the lossy fallback precisely where it is needed most.
- **A size-keyed budget**, as originally proposed - but note the diagnosis at the top of this
  section: residual size predicts APTED cost only loosely (26.9M-product call: 5.5ms;
  391k-product call: 1154ms), so such a budget fires on the wrong calls. This is the third
  independent time this project has found a size proxy unreliable (see also 2026-07-25).

Given all of the above, the honest recommendation is to **stop pushing p99 through APTED tuning**
and revisit whether the p99<400ms target should cover the deliberately-gigantic stress fixtures at
all (an open question flagged in "Architecture rethink" above and never resolved). 11 fixtures now
exceed 400ms; several are 75k-258k-node files where a few hundred ms is arguably correct behaviour,
not a defect. Quality work (the 66 near-miss fixtures) has a far better return per unit of risk
from here.

### #1 implemented: `solve_unique_type_matching` - zero firings, a real negative result (2026-08-17)

Shipped as `src/diff/solve_unique_type_matching.rs`, gated by `HeuristicConfig::solver_unique_type_
matching` (default `true`), wired in right after the first `solve_bottom_up_propagation` call and
before the terminal `apted::for_roots_fallback`. Unit-tested in isolation (two tests that pre-match a
node directly via `diff.add_mapping`, not via an earlier real pass, so a passing assertion can only
be this pass's own doing - an earlier draft's tests pre-matched the *outer* function instead of the
block its children live under, which let real APTED resolve the target node as a side effect of a
different match and made both tests pass without the mechanism under test ever running; caught by
checking `ASTMappingReason` on the resulting mapping, not just checking a match exists).

**Full-corpus measurement (417 fixtures, `--solver-unique-type-matching` vs. `--no-solver-unique-
type-matching`): byte-for-byte identical output, 2835/2835 total mismatches, 310/417 (74.3%)
zero-mismatch either way.** Confirmed via temporary `CODEDIFF_DEBUG_UNIQUE_TYPE`-gated
instrumentation (added, used, fully reverted - `git diff` clean beyond the intended change) that the
pass fires **zero times** across the entire real corpus, even though the isolated unit tests prove
the mechanism itself is correct. Root cause: by the point in the pipeline where this pass runs, `Kind
OnlyHash` sub-anchoring (`solve_hash_descent`, shipped well before this session) has already resolved
essentially every "same shape, different leaf value" case GumTree Simple's own earlier recovery
sub-phases (exact isomorphism, structural isomorphism ignoring leaf values) target - and codediff's
other passes are comprehensive enough that a matched parent's leftover unmatched children essentially
never land on the exact "one of this kind on each side" shape this pass needs: either every child is
already resolved by something else, or several share a kind and the ambiguity guard correctly blocks
it. This is the same shape of negative result as Task #8 ("reposition move-detection twice") above -
a literature-backed, plausible-sounding idea that is safe and correctly implemented but has no
customer in *this* corpus, given everything upstream of it in *this* pipeline. Kept shipped
(zero measured risk, since it never fires) rather than reverted, in case a future pipeline reordering
or corpus addition gives it a customer - but this is not a case to cite as "implemented and helping."

## Phase 1: Quick Wins (1-2 weeks, production-ready)

### Commutative Sibling Matching

**STATUS (2026-08-17): substantially shipped, this entry is stale.** `nodes::is_commutative_container`
+ `code::hash`'s `compute_kind_and_value_hash`/`compute_kind_only_hash` already give order-independent
hashing to every language-guaranteed-order-independent container (struct/field lists, enum variants,
import lists, JSON/YAML object keys, Python dicts, JS/TS objects - see `nodes.rs`'s full match arm).
The originally-proposed "sort by canonical key before hashing" design was superseded by folding
order-independence directly into the hash functions instead. What's left is a genuinely different,
harder problem - order-*sensitive* constructs (if/elseif chains) that a human reordered anyway - see
this file's "Literature + external-implementation survey" section above (2026-08-17) for why that
needs branch-level move detection, not a blanket commutativity rule, and is not yet attempted.

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
| 1 | Commutative sibling matching | ⭐⭐⭐⭐ | Medium | High | **DONE for language-guaranteed-commutative kinds (`is_commutative_container`); if/elseif branch-move case still open, see 2026-08-17** |
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

## Phase 6 history (moved out of `PendingDiff::finish` on 2026-09-05)

The comment below sat inline above the phase-6 call in `diff.rs` until the 2026-09-05 code
health pass; it is the record of why the whole-residual full-APTED pass went away and what that
cost at the time. `DiffMode`/`--exact`, which it refers to, were removed in that pass - `finish`
takes no mode, and JSON output's `fallback_used` became `large_residual`.

Phase 6: pre-match top-level scope-locally-named entities (e.g. shell variable
assignments with no enclosing named container at all, so `solve_syntax_aware_matching`'s
own call to this never fires for them) whose name is unique and survives a position
shift caused by an unrelated insertion elsewhere in the file - see
`apted::prematch_unique_named_locals`'s doc comment ("shift-due-to-insertion") - then
resolve the whole-file residual via the cheap Myers-LCS-based fallback
(`apted::for_roots_fallback`), unconditionally, regardless of `_mode`.

Until the phases-4-7 rearchitecture (`TODO.md`, `~/.claude/plans/iterative-herding-
panda.md`), this branched: `DiffMode::Exact` (or `Fast` below `EXPENSIVE_RESIDUAL_
THRESHOLD`) ran unconditional whole-residual full APTED (`apted::for_roots(...,
Algorithm::Apted, "final_pass", ...)`) instead. That call is deleted as of this commit
(Step 1 of the rearchitecture, not to be confused with this file's own outer Phase 1):
its Θ(n1×n2) dense-matrix cost is driven by residual
shape, not size, and cannot meet the project's p99<400ms target no matter how it's
gated - see the measured pathology (`vimscript-neovim-...add-two-functions`: 87s at
11,647 nodes, vs. `json-iwalton3-jellyfin-web-...`: 9.4s at 258,504 nodes) recorded in
`TODO.md`'s "Architecture rethink: target goals" section.

MEASURED QUALITY COST AT THE TIME (2026-08-15, see TODO.md): this alone regressed
249/257 fixtures that then relied on real APTED (175 dropped from 0 mismatches to
nonzero, net +4880 mismatches on that subset) - `resolve_residual_forest_via_myers_lcs`
only recovers whole-subtree byte-identical matches, so any residual with even one
genuinely-changed node lost partial credit for everything around it. This is why the
change first landed on the `phases-4-7-rearchitecture` branch rather than `main`
directly: Steps 2-3 of the rearchitecture (bottom-up propagation, region-scoped real
APTED dispatch) had to
land alongside it first, replacing what real tree-edit-distance-quality matching this
call provided with bounded, per-region matching.

STATUS (2026-08-17): that branch is merged into `main` (`git merge-base --is-ancestor
phases-4-7-rearchitecture main` confirms it) - this is not branch-only behavior, it is
what `main` does today. Quality is back at or above the pre-Phase-1 baseline (see
`research/data/quality/optimal_solutions_benchmark.csv` and TODO.md's later 2026-08-16/17 entries:
kind-only sub-anchoring, trivial-entry filtering) - the regression described above was
real but transient, measured mid-migration before Steps 2-3 of the rearchitecture
shipped, not a standing
cost of this design.

`prematch_unique_named_locals` now runs unconditionally too (previously only in the
deleted `else` arm) - measured in isolation and found NOT to be a meaningful
contributor to the regression above (nearly identical delta with/without it: +4880 vs
+4860) - the fallback's own lossiness dominates.

`DiffMode`/`_mode` no longer changes behavior - kept on the signature for API
compatibility (the `--exact` CLI flag still constructs one; the TUI's old
`SelectDiffMode` prompt was removed in the 2026-08-19 usability pass) pending a
follow-up commit that decides whether to remove it now that quality has recovered.

---

*Last updated: 2026-09-05*
