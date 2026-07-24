# Major pipeline rework (SHIPPED, 2026-07-17/18 - old pipeline fully retired)

## Locality tie-break in `solve_moved_subtrees` - tried 2026-07-22, reverted, zero effect; plus a cost-comparison analysis of the 800-mismatch corpus

Follow-up to the 2026-07-22 session that landed 7 fixes (919 -> 800 mismatches, commit `d74667a`).
Investigated whether a locality-based tie-break (`preorder_index`/`start_byte` proximity) could
close more of the gap, motivated by a design discussion about fitting the cost function so the
human-authored mapping is provably optimal (structured/margin-based learning framing).

**First considered**: adding a small positional tie-break term directly inside
`apted::common::UnitCostModel::ren`. Ruled out before writing it, once the actual blast radius was
mapped: `COST_INSERT`/`COST_DELETE`/`COST_UPDATE` are read directly (not just via `ren`) by ~20
existing tests that hardcode exact expected costs, so any rescale to make integer headroom for a
tie-break would require updating all of them - and worse, the bugs motivating this (APTED
preferring a byte-identical-but-distant node over the correct nearby one) live entirely in `ren`'s
*0-cost* tiers (`Identical` leaf match, same-kind internal match). Those tiers must stay exactly 0
- a subtree can legitimately relocate to a very different absolute file position and still be
100% correctly "the same code, just moved" (that's what `COST_MOVE = 0` encodes, and what many
tests assert: `test_rust_add_if`'s `assert_eq!(mapping.cost, 23)` relies on the entire reused
if/else costing exactly 0 despite moving one level deeper). A pointwise cost function can't tell
"legitimately relocated" apart from "coincidentally identical to something unrelated" - it only
ever sees one candidate pair at a time, never the competing alternatives. Any tie-break large
enough to matter would make legitimate whole-subtree moves stop costing 0.

**Pivoted to**: `hash_tree_matching.rs` already establishes exactly this tie-break pattern at every
one of its candidate-selection sites (`solve_with_hash_map`, both tiers of
`pair_children_for_descent`) - `min_by_key` on `start_byte().abs_diff(source.start_byte())` among
same-hash candidates. `solve_moved_subtrees.rs` was the odd one out: its candidate sort broke ties
by absolute earliest-in-file position (`sort_unstable_by_key(|a| ... start_byte())`), not proximity
to the deleted node's own position - inconsistent with the established convention and a plausible
match for the "byte-identical-elsewhere preferred" failure mode. Changed it to
`start_byte().abs_diff(b_start_byte)`, matching the sibling sites exactly.

Validated: full `cargo test --lib` (356/356 passed), full `benchmark_optimal_solutions` with a
proper per-fixture CSV comparison (not just aggregate - see `TODO.md`'s standing lesson about
that). Result: **zero effect**, at every level - aggregate 800 -> 800, 0 fixtures improved, 0
regressed, and the total count of `Moved`-reason mappings across the *entire* corpus was identical
before/after (236 -> 236). Root cause: `solve_moved_subtrees`'s existing container-identity-
agreement filter (outermost unmapped reference-node kind must match) already narrows candidates to
at most one survivor in every case in this corpus - the ambiguity this tie-break targets never
actually arises here. Also corrected a stale claim: `cpp-ladybird-refactor-variables-if-changes`'s
109 mismatches are `APTED:final_pass`/`APTED:greedy_anchor_block`-attributed, not `Moved` - it was
never `solve_moved_subtrees`'s doing. Reverted cleanly (`git checkout -- src/diff/solve_moved_
subtrees.rs`, confirmed clean via `git status`). The fix is still *correct* (consistency with the
established tie-break convention, zero risk) - just not reachable on this corpus. Worth re-applying
if a future corpus fixture actually exercises the ambiguity.

**Cost-comparison analysis** (the more useful output of this round): asked whether the human
ground-truth mapping is itself always "correct," given APTED finds a provably cost-optimal mapping
under whatever cost model it's given - if the human's mapping isn't the unique optimum, a
"mismatch" may just be a different, equally-valid tie-break rather than a codediff defect. Checked
this directly using `benchmark_optimal_solutions --csv`'s `algorithm_cost`/`human_cost`/`cost_diff`
columns (mirrors `UnitCostModel`'s costs via `cost.rs::operation_cost`, independent of the tie-break
work above) across all 26 mismatched fixtures (800 total mismatches):

- **320 mismatches (40%), concentrated in 4-6 fixtures** - codediff's mapping costs *the same or
  less* than the human's own, e.g. `cpp-ladybird-refactor-variables-if-changes` (algo 660 vs. human
  711), `kotlin-refactor-function` (100 vs. 162), `kotlin-remove-function` (168 vs. 225), `cpp-
  laydbird-change-function-signature` (174 vs. 192). For these, the human's answer is demonstrably
  *not* the unique cost-optimal mapping - a cheaper or equal alternative provably exists (codediff
  found it). Calling these "wrong" is questionable; they may be legitimate alternative optima the
  human just didn't happen to pick.
- **480 mismatches (60%), across 20 fixtures** - codediff's mapping is *strictly more expensive*
  than the human's, even by the algorithm's own yardstick, e.g. `csharp-jellyfin-add-function` (209
  vs. 117), `c-postgres-real-logic-change` (484 vs. 49), `rust-turbopack-module-rule` (601 vs. 424).
  This is the more actionable bucket: if a strictly cheaper alternative demonstrably exists (the
  human's), codediff isn't finding *its own* optimum, let alone the human's - which traces back to
  the pipeline's architecture, not a cost-model tie-break. Earlier greedy phases (hash descent,
  bottom-up expansion, syntax-named matching) commit matches before the true-optimal APTED
  (`final_pass`) ever sees the residual, so the overall pipeline result is not globally
  cost-optimal even though each individual phase is locally sound. This is where real, provable
  headroom remains; the 40% bucket above may be an irreducible floor unless the cost model is
  taught the human's specific tie-break preference (which is exactly the harder problem the ruled-
  out `ren` approach ran into).

## `final_pass` forced-root-pairing cost gate - tried 2026-07-18, reverted, net-negative

Follow-up investigation after the phase 4 expansion candidates above: examined the top-6
highest-mismatch fixtures in the 778-mismatch corpus and found two distinct, separable root
causes behind most of their failures (full analysis not otherwise written down - see this entry
for the summary). **Root cause A**: real APTED tree-edit-distance, when a large region is
extensively rewritten, has no secondary objective to prefer preserving small byte-identical
islands over an equal-or-near-equal-cost alternative alignment (`rust-zed-workspace-tasks`'s
`self.terminal_provider` dropped inside a heavily-rewritten `if` block). **Root cause B**: a node
the human ground truth says should be deleted outright instead gets matched to an unrelated
same-kind node elsewhere, because `final_pass` (`apted::for_roots`, phase 6's whole-file-residual
catch-all) diffs exactly one file root against exactly one file root unconditionally, and
`island_match_supported`'s "immediate parent is matched" context-validation shortcut is trivially
true for *every* top-level declaration once the file root is (necessarily) matched to the file
root - "the file matches the file" is forced, not evidenced, but the validation code can't tell
that apart from a real evidenced anchor (e.g. `syntax_named`'s own name-matched forest roots).

Prototyped a fix for root cause B only (lower-risk of the two, and directly actionable): added
`is_forced_root_pairing` (`src/diff/apted/common.rs`) - true when a candidate match pair's parents
are both exactly the forest roots of a `source == "final_pass"` resolution - and gated on it in
two places: `island_match_supported` (withholds the trivial "parent matched"/"nearby matched
ancestor" shortcuts there, requiring either byte-identical content or a `solve_greedy_anchor_
blocks::cost_ratio` bound instead), and `promote_same_slot_pairs`'s LCS `weight` closure (same
cost-ratio requirement for internal same-kind slot promotions under a forced root pairing - this
turned out to be **necessary**, not optional: `island_match_supported` alone left the exact same
wrong pairing to be silently re-introduced by `promote_same_slot_pairs`'s independent "same slot,
so this must be an edit" mechanism one step later in `improve_slot_alignment`, since that
mechanism has its own, much weaker escape hatch - `LARGE_SLOT_SUBTREE`/`share_descendant_hash` -
that only blocks promotion for bodies over 20 nodes with *zero* shared descendant hash, which real
mismatched functions routinely have by coincidence).

Two new unit tests (`final_pass_does_not_match_unrelated_top_level_functions`, `final_pass_still_
matches_renamed_function_with_modest_body_change`) passed - the mechanism worked exactly as
designed on synthetic small functions. But full lib suite: **32 failures**, many in previously-
*exact* (0-mismatch) fixtures (`python-added-if-block`, `java-add-logging`, several `cpp-*`/
`typescript-*` ones). Benchmark: **TOTAL 832 vs. the 778 baseline (+54), and none of the 6
originally-targeted fixtures improved at all** (`kotlin-refactor-function`/`kotlin-remove-function`
unchanged, `cpp-ladybird-refactor-variables-if-changes`/`cpp-laydbird-change-function-signature`/
`c-nginx-add-typedef` slightly *worse*) - a pure net-negative with zero offsetting benefit even on
its own target fixtures.

Two things went wrong, both real lessons for next time:
1. **The cost-ratio threshold was too blunt for legitimate large single-candidate edits.**
   `python-added-if-block`'s regression is the clearest case: a lone top-level `if_statement` had
   a whole new nested `if` added inside it - a real, unambiguous edit (nothing else on either side
   could plausibly be its counterpart) - but `solve_greedy_anchor_blocks::cost_ratio`'s coarse
   whole-direct-child-hash-equality comparison (the same limitation candidate #3/#4's `TODO.md`
   entries above already flagged for calls) scored it as too expensive and rejected the match,
   converting a correct identity match into a wrong delete+insert. The gate has no way to
   distinguish "this pairing is genuinely ambiguous, cost-check it" from "this is the only
   candidate on either side, cost is irrelevant" - it should have been skipped entirely whenever
   there's no competing candidate to be wrong *about*.
2. **The targeted fixtures likely never even reached the modified code paths.** None of the 6
   fixtures the whole investigation was aimed at moved at all (not even in the wrong direction,
   which would at least confirm the gate was reachable and just mistuned) - the more likely
   explanation is `resolve_forest`'s flat-tree fast path (`flat_children`, `FLAT_MIN_CHILDREN =
   50`: files whose root has 50+ unmatched direct top-level children route through Myers O(ND)
   sequence diff instead of `compute_edit_mapping`/`improve_slot_alignment` entirely) intercepted
   these larger, real-world files before `island_match_supported`/`promote_same_slot_pairs` ever
   ran, meaning the actual root-cause-B mismatches diagnosed in those fixtures are produced by a
   different mechanism than the one this fix touched. Not confirmed further - reverted before
   chasing that down.

Reverted cleanly (`git checkout -- src/diff/apted/common.rs src/diff/apted/common/tests.rs`,
confirmed zero diff, lib build green). Revisit only with both lessons addressed: (a) skip the
cost-ratio requirement entirely when there is no competing candidate on either side (only real
ambiguity needs adjudicating), and (b) first confirm which code path *actually* produces the
kotlin-refactor-function/cpp-ladybird-style mismatches - likely by instrumenting or by checking
whether those files' root child counts cross `FLAT_MIN_CHILDREN` - before spending more effort
tuning a gate that may not even be reachable for the fixtures it's meant to fix.

## Phase 4 expansion candidates (PLANNING, 2026-07-18 - being tried one by one)

User request after the phase 4 generalization above: "what are some other similar heuristics or
expansions we can do to phase 4?" then "write them all in todo and try them one by one." Four
candidates, in the order they'll be tried (roughly confidence-ordered, cheapest/lowest-risk
first). Each gets the same treatment every heuristic in this codebase's history has: implement,
`cargo test --lib`, full `benchmark_optimal_solutions` run against the current baseline, keep only
if it helps or is neutral - **the ablation history in this file is full of heuristics that looked
reasonable and turned out net-negative alone** (`solve_similar_flow_control`, `solve_bottom_up_
expansion`, import-path hashing all shipped anyway because they're net-positive *in combination*,
but that was only known because each was actually measured, not assumed).

1. **Field/variant-level named matching** - **tried 2026-07-18, reverted, net-negative.** Extended
   `solve_named_reference_groups`'s candidate predicate with a new `nodes::is_named_field`
   (Rust `field_declaration`/`enum_variant`, Go `field_declaration`, Java `field_declaration`, C/C++
   `field_declaration` when not pointer/array-wrapped - each verified against real grammar output,
   4 passing unit tests). Worked exactly as designed in isolation (new end-to-end test confirmed
   `age: i32` matches by name across sibling field churn) but regressed the benchmark: **785 vs. the
   778 baseline (+7), 3 fixtures worse (`rust-add-value-to-enum` 0->2, `rust-hash-optimization`
   0->2, `rust-turbopack-module-rule` 52->55), 0 improved anywhere.**
   **Root cause, confirmed via `--details` on all 3 regressions - the same failure mode every
   time:** calling `apted::for_nodes` on a single matched field/variant pair causes that call's own
   comma assignment (commas are *siblings* of `field_declaration`/`enum_variant` in their parent
   list, a classic generic/interchangeable token) to disagree with how the *parent* list
   (`field_declaration_list`/`enum_variant_list`) would have assigned commas holistically across
   all its children at once. Matching one field in isolation fragments a decision that needs
   list-wide context to get right - individual field identity and the list's own generic-token
   assignment are in tension, and this implementation had no way to reconcile them. Every
   regression was a comma-only mismatch (not a real content/structure error), so this is milder
   than it looks, but it's still a measured net-negative by the strict TOTAL count this whole
   session has used as the bar. Not chased further (would need either scoping `apted::for_nodes`
   to include the parent list's generic tokens, or gating the predicate away from fields whose
   parent is a `nodes::is_commutative_container` where this tension is sharpest) - reverted
   cleanly (`git checkout`), zero trace left in the codebase. Revisit if a future session wants to
   solve the underlying generic-token-in-isolated-APTED-call problem generally, since it would
   likely also matter for future candidates like #3/#4 below.

2. **Import-list arm-overlap matching** - **tried 2026-07-18, kept, neutral.** `solve_import_
   list_overlap`: groups candidate Rust `use_declaration`s (multi-symbol `use foo::{a, b, c}` form
   only) by base import path, scores same-path pairs by Jaccard similarity of imported-symbol sets
   (reusing `solve_similar_flow_control`'s generic `flow_control_similarity_of_sets` helper),
   matches the *whole* `use_declaration` in one `apted::for_nodes` call (never an individual
   symbol - applying #1's lesson directly). Benchmark: **TOTAL 778 exactly unchanged, zero
   fixtures differ** - the new `APTED:syntax_import_list` reason fired 76 times with no net effect
   on mismatches. Kept anyway (same call as `solve_greedy_anchor_blocks` previously): zero
   regression, 2 passing unit tests, plausibly useful on real-world grouped-import churn this
   86-fixture corpus doesn't happen to exercise.

3. **Call-site matching by callee identity** - **tried 2026-07-18, reverted, zero firings.**
   `solve_call_site_by_callee`: grouped still-unmatched Rust `call_expression` candidates by
   `(callee name, argument count)`, cost-scored by how much of the *argument list* specifically
   changed (`solve_greedy_anchor_blocks::cost_ratio` applied to the two candidates' `arguments`
   nodes), narrowed via `CALL_SITE_MIN_ARGS`/`CALL_SITE_MIN_SUBTREE_SIZE`/`CALL_SITE_MAX_COST_RATIO`
   to avoid the generic-callee-collision risk flagged below, matched the whole `call_expression` in
   one `apted::for_nodes` call (applying #1's lesson). Compiled clean, 1 new unit test passed
   end-to-end (`process_item(a, b, 1)` matched across both a position shift and an arg-value
   change), full lib suite green (334 passed). Benchmark: **TOTAL 778 exactly unchanged, zero
   fixtures differ - but unlike #2, `APTED:syntax_call_site` fired 0 times anywhere in the 86-fixture
   corpus.** Distinct outcome from both #1 (measured regression) and #2 (fired 76x, neutral): this
   is *unvalidated*, not *validated-neutral* - the pass never ran on real input, so its flagged
   false-positive risk (common callee names colliding) was never actually exercised either way, and
   it would have shipped unconditionally (not behind a config gate). Reverted cleanly (`git
   checkout`, confirmed zero diff) rather than keep unexercised complexity with no measured benefit.
   Zero firings across ~15-20 Rust fixtures is itself mild evidence this specific edit pattern
   (callee unchanged, but call moved position *and* had args edited) is rare in practice, which
   undercuts the original "useful on real-world code this corpus doesn't happen to exercise"
   argument that justified keeping #2. Revisit only if a future fixture actually demonstrates the
   need - don't resurrect by loosening thresholds, since that trades away the false-positive
   protection to manufacture firings.

4. **Named/keyword-argument matching** - **tried 2026-07-18, reverted, zero firings.** The
   originally-proposed framing ("match a keyword argument independent of position within a call")
   is exactly candidate #1's individual-list-member form one level further down the tree, and would
   hit the identical comma-fragmentation failure #1 measured. Adapted instead (advisor-reviewed
   before implementing) into `solve_keyword_argument_calls`: group still-unmatched Python `call` /
   Kotlin `call_expression` candidates (verified against each grammar directly - Python has real
   `function`/`arguments`/`name`/`value` fields, Kotlin's `tree-sitter-kotlin-ng` grammar has *no*
   fields at all on `call_expression`/`value_argument`, so callee and keyword-name extraction there
   is purely positional) by `(callee name, sorted keyword-argument-name set)` - a stronger,
   more collision-resistant identity signal than candidate #3's `(callee, arg count)`, since
   keyword names are caller-chosen and self-describing. Matches the *whole* call in one
   `apted::for_nodes` call, never an individual argument (applying #1's lesson). Hit and fixed one
   real bug along the way: `solve_greedy_anchor_blocks::cost_ratio` only compares a node's *direct*
   children by exact hash, and a call's direct children are just `(callee, arguments)` - scoring on
   the whole call read one changed keyword value as "100% different" (ratio 0.91, above any sane
   threshold) regardless of how much the argument list actually shared; fixed by scoring on the
   *arguments* sub-node specifically instead (same fix candidate #3 already needed and documented).
   Both a Python and a Kotlin unit test pass end-to-end (call matched across a position shift plus
   one changed keyword value), proving the mechanism works when the pattern is present. Benchmark:
   **TOTAL 778 exactly unchanged, zero fixtures differ - but `APTED:syntax_keyword_args` fired 0
   times anywhere in the 86-fixture corpus** (which does include multiple Python and Kotlin
   fixtures), the same zero-firing outcome as candidate #3. Reverted cleanly (`git checkout`,
   confirmed zero diff) for the same reason: unconditional, always-on code with no measured benefit
   and an unexercised false-positive-collision risk shouldn't ship just because it's mechanically
   correct. Revisit only if a future fixture demonstrates the need - not by loosening thresholds.
   The narrower position-independent-*within-an-already-matched-call* gap (this candidate's
   original framing) stays genuinely unaddressed, same deferred status as candidate #1's fragmented-
   comma-token problem - solving both together (matching a single list member without breaking the
   parent list's own generic-token assignment) is the real unlock, and remains future work.

   **Phase 4 expansion candidates: final tally (2026-07-18).** All four tried one by one, each
   measured independently against the 778 baseline: **#1 reverted** (measured regression, 785).
   **#2 kept** (neutral, fires 76x on real fixtures). **#3 reverted** (neutral, 0 firings -
   unvalidated). **#4 reverted** (neutral, 0 firings - unvalidated). Net result: one net-positive
   addition shipped (`solve_import_list_overlap`), three speculative extensions tried in good faith
   and cleanly discarded when the evidence didn't support keeping them. No open follow-up work is
   in flight from this exercise - the deferred generic-token-in-isolated-APTED-call problem (#1's
   root cause, also blocking a real fix for #4's original framing) is noted above but not scheduled.

## Phase 4 generalized into one shared engine (2026-07-18)

User question: "what is the generalization of all current phase 4 heuristics?" Answer: three of
phase 4's four mechanisms - named-group matching, positional anchoring (`solve_greedy_anchor_
blocks`), and flow-control arm-overlap (`solve_similar_flow_control`) - are the same algorithm:
partition candidates into buckets by an exact compatibility key, score same-key pairs with a cheap
cost function, greedily accept cheapest-first (optionally above a threshold), hand accepted pairs
to real APTED. They differed only in three pluggable pieces (candidate predicate, key type, cost
function). `solve_large_flat_subtrees` (the fourth mechanism) doesn't fit this shape at all - no
competing candidates, no scoring, just a deterministic lookup pre-empting part of an already-
established pair's own APTED call - and stays a separate, directly-called pass.

Built `src/diff/grouped_greedy_matcher.rs` as the one shared engine (mirrors how `hash_tree_
matching::solve_with_hash_map` already generalized phase 1's three hash algorithms the same way)
and re-pointed all three call sites at it. Notable transform: flow-control's Jaccard similarity
("higher is better") had to be converted to the engine's "lower cost is better" convention via
`1.0 - similarity`, both for the cost function and the threshold. `FlowControlFamily` gained a
`Hash` derive to serve directly as the engine's compatibility key.

Verified: full lib test suite green (331 tests, including positional-anchoring's false-positive
regression guards and the N:M overload test), benchmark re-confirms **TOTAL 778 exactly** - pure
refactor, zero behavior change. Unexpected bonus: aggregate benchmark runtime dropped from ~327s to
**175s** (nearly 2x), because flow-control's previous O(before x after) all-pairs family filter now
benefits from the same O(candidates) key-bucketing the other two mechanisms already had.

## Final cleanup (2026-07-18)

Old ~15-pass pipeline deleted outright. `Diff::from_code_with_config` now runs the seven phases
directly - no more `HeuristicConfig::use_new_pipeline` toggle, no more parallel branch. Deleted:
`solve_identical_trees.rs`, `solve_structurally_identical_trees.rs`, `solve_commutative_structural_
trees.rs`, `solve_multilevel_hash.rs`, `solve_semantically_structural_nodes.rs`, `solve_import_
nodes.rs` (its 4 reused helpers moved into `solve_hash_descent.rs`). `HeuristicConfig` shrank to
the 4 fields that still gate a real decision (`solver_import_nodes`, `solver_similar_flow_control`,
`solver_bottom_up_expansion`, `solver_moved_subtrees`); the 4 now-unproducible `NormalizedStructural
Ignore*` reason variants and their backing `ASTMetadata` hash fields were removed too, along with
the now-fully-unused `semantically_structural_nodes` metadata field (phase 4 walks the tree
directly instead). `ablation_study.sh` and `benchmark_optimal_solutions.rs`'s CLI flags updated to
match (14 flag pairs -> 4). Verified: full workspace build clean, 331 lib tests green, benchmark
re-confirms **TOTAL 778** exactly - identical to the last pre-cleanup run, zero behavior change.
Net -2,345 lines. This closes out the rework - see the sections below for the full design history.

## Implementation status (2026-07-17)

All six phases implemented and wired end-to-end behind `HeuristicConfig::use_new_pipeline` /
`benchmark_optimal_solutions --new-pipeline`, built **alongside** the old ~15-pass pipeline rather
than replacing it in place (a deliberate deviation from the "clean replacement, no A/B period"
framing below - added specifically so the new pipeline could be benchmarked against the old one
throughout the rework instead of flying blind; see the advisor consultation this session for why).
Old pipeline, all 338 lib tests, and the 782-mismatch baseline are all unaffected and still pass.

New files: `src/diff/solve_hash_descent.rs` (phase 1 orchestration), `src/diff/solve_syntax_aware_
matching.rs` (phase 4). Modified: `code/hash.rs` (+`compute_kind_and_value_hash`/`compute_kind_
only_hash`, order-independent at every recursion level - the propagation-bug fix described below,
now actually applied), `diff/hash_tree_matching.rs` (+`solve_with_hash_map`, the generalized
engine), `diff.rs` (+`Diff::run_new_pipeline`), `diff/solve_greedy_anchor_blocks.rs` (`cost_ratio`
widened to `pub(crate)`, reused by phase 4's named matcher), `diff/solve_import_nodes.rs` (3 helpers
widened to `pub(crate)`, reused by phase 1's import-path hash variant).

**A real correctness bug was found and fixed along the way**, independent of the benchmark:
`solve_with_hash_map`'s descendant pairing was a positional `zip`, which mis-pairs children once a
commutative container's reordering is hash-matched (exactly the failure mode `compute_commutative_
structural_hash`'s own doc comment warned about, quoted below). Fixed by pairing commutative-
container children by `kind_only_hash` with a document-proximity tiebreak instead of position - see
`pair_children_for_descent` in `hash_tree_matching.rs`.

**Benchmark result, apples-to-apples** (86-fixture `optimal_solutions` corpus, `--new-pipeline`
with the same 3 passes gated off that `HeuristicConfig::default()` already gates off - import-path
hash, `solve_similar_flow_control`, `solve_bottom_up_expansion`'s second call - since those were
found net-negative in the 2026-07-15 ablation study and nothing about the rework changes that
signal): **888 mismatches vs. the 782 baseline** (+106, +14%). Before gating those 3 (i.e. running
everything unconditionally) it was 1067 - close to the old pipeline's own *all-passes-on* number
(~1022 per the ablation note further down this file), confirming the new pipeline's baseline
behavior is architecturally sound, not fundamentally broken.

**Root cause of the +106 gap, diagnosed via `--details` and reason-count column comparison, not
guessed:**
- The two biggest single-fixture regressions - `c-cpython-autogenerated-code` (58 -> 141, +83) and
  `rust-turbopack-module-rule` (52 -> 86, +34) - are **not** a bug. Both fixtures' old-pipeline
  reason-count row has a nonzero `Moved` column (103 and 55 matches respectively, from
  `solve_moved_subtrees` recovering byte-identical wholly-deleted+wholly-inserted subtree pairs -
  i.e. genuinely duplicated/reordered code, plausible for cpython's autogenerated opcode tables and
  a renamed-impl rename in turbopack). The new pipeline's row has no `Moved` column at all, by
  design: **the user explicitly confirmed removing `solve_moved_subtrees` outright** ("remove
  solve_moved_subtrees") when this phase 2 was originally scoped. This is that decision's real,
  measured cost on 2 fixtures - not a defect in the new code. Whoever picks this up next should
  decide whether to accept this tradeoff (matches the explicit instruction) or reconsider carrying
  some move-detection safety net forward in a different form.
- One fixture improved substantially: `cpp-laydbird-change-function-signature` (75 -> 23, -52) -
  the fully-resolved-name matcher's flat, uniform mechanism apparently handles this one better than
  the old two-tier impl/class pre-pass did.
- A long tail of smaller *new* regressions (1-28 mismatches each) appeared across roughly 15
  fixtures that were previously exact (0 mismatches) or near-exact under the old pipeline (e.g.
  `c-linux-small-bugfix` 0->28, `cpp-godot-small-bugfix` 0->11, `csharp-sonarr-change-type` 0->11,
  `csharp-jellyfin-sql-query-fix` 0->10, plus ~10 more at 1-8 each). **Not yet individually root-
  caused** - time-boxed out of this session. Leading suspects for whoever investigates next, in
  likely-impact order: (1) `KindOnlyHash` folding `solve_multilevel_hash`'s 4 normalized granularities
  (punctuation-only, literal-only, identifier-only, punctuation+literal) into one coarse tier - an
  *accepted* precision loss per the plan below, but its accuracy impact was explicitly flagged as
  "worth checking via `benchmark_optimal_solutions` once implemented" and this is that check landing
  non-trivially non-zero; (2) the old `solve_semantically_structural_nodes`' `pre_match_by_path`
  pre-pruning (matching identical/structurally-identical children before invoking APTED on a named
  pair, shrinking the residual) has no equivalent in the new phase 4's named matcher - every accepted
  name-based pair goes straight to a full `apted::for_nodes` call on the whole subtree.
- One already-attempted, confirmed-ineffective fix: re-adding an explicit `solve_large_flat_
  subtrees::solve` call at the top of phase 4 (matching the old pipeline's ordering) made zero
  measured difference (888 -> 888 exactly, `c-cpython` unchanged at 141) - because `nodes::is_
  semantically_structural` (which both the old and new large-flat-subtree top-level-identity lookup
  depend on) doesn't cover C at all, so the pass was already a no-op for `c-cpython` in *both*
  pipelines. Left in the new phase 4 anyway (it's harmless and does help other languages/kinds it
  does cover), but it is not the fix for the `c-cpython`/`turbopack` regressions - see `Moved` above.

**Not yet done**: the "final cleanup" step (flip `use_new_pipeline` default to `true`, delete the
~9 superseded modules/reasons/config fields, remove the toggle) is explicitly gated on closing this
gap first - see the disposition table's note that this was always meant to happen last, after
verification, not as part of "clean replacement" meaning "delete-first."

## Phase 7: unanchored-move fallback (added 2026-07-18)

User request after reviewing the `solve_moved_subtrees` vs. phase 4 comparison above: add back a
dead-last fallback for content that *moved* between two containers that both changed identity -
the one case none of phase 4's four mechanisms can reach, because every one of them requires an
anchor (a hash, a name, a shared matched ancestor, an arm-signature overlap) and a true cross-tree
relocation has none of those on either side until the rest of the pipeline, including final APTED,
has already given up. This does **not** reopen the "remove `solve_moved_subtrees`" decision from
the original planning - that removal was about phase 2 ("contextual exact matching", repurposed
to mean something else, see above); phase 7 reuses the `solve_moved_subtrees` module itself, called once
more, in its original dead-last position, now the pipeline's 7th and final step. Gated on
`config.solver_moved_subtrees`, same knob the old pipeline uses (default `true`).

**Result: 789 mismatches vs. the 782 baseline (+7, +0.9%)** - both target fixtures fully recovered
to their exact old-pipeline counts (`c-cpython-autogenerated-code` 141 -> 58, `rust-turbopack-
module-rule` 86 -> 52). A full old-vs-new per-fixture diff of all 86 fixtures now shows **exactly
one** remaining difference: `rust-next-font-imports-generator` (22 -> 29, +7 - the entire remaining
gap). `--details` on that fixture shows the extra mismatches are all "codediff chose Identical,
human mapping expected MatchButNotIdentical" on a reordered `use_list` (Rust's commutative-container
kind) and a restructured `if`-chain - confirmed (see next section) to be the commutative-hash
propagation-bug fix correctly recognizing the reordered import list as unchanged, which the
recorded human ground truth doesn't credit.

## Distinguishing reordered from truly identical (added 2026-07-18)

User request: "we do need a way to distinguish between truly identical and reordered." Confirmed
the diagnosis above was right, and that it's a real, general gap: folding order-independence
directly into `KindAndValueHash`/`KindOnlyHash` fixed the propagation bug but also meant a
reordered-but-otherwise-unchanged commutative container is indistinguishable from a genuinely
untouched one - both get plain `IdenticalHash`/`IdenticalHashOfAncestor`. The old, now-removed
`solve_commutative_structural_trees` kept this distinguishable via its own dedicated reason
(`FullymappingSubtrees`); the new hash-descent engine had no equivalent.

Fix: `ASTMappingReason::FullymappingSubtrees` (already existed, was slated for removal, **not
removed after all** - repurposed) now gets applied by the new engine too.
`hash_tree_matching::pair_children_for_descent` returns a `reordered` flag (true if any child's
after-side document-order index differs from its before-side index within a commutative parent),
and `solve_with_hash_map` patches that parent's already-added mapping to `FullymappingSubtrees`
once its children are examined. Operation/cost are unchanged (reordering-only stays free, matching
the old pass's precedent) - only the *reason* now differs, so downstream consumers (a diff viewer,
`human_solver`, `benchmark_optimal_solutions`'s reason-count columns) can tell the two cases apart.

Fixing this surfaced a second, independent bug in the same function (introduced earlier this
session, not by the original commutative-pairing fix's intent): child pairing always used
`node_to_kind_only_hash`, which is too coarse whenever the *outer* match came from
`KindAndValueHash` - same-kind, different-value leaves (e.g. three plain `identifier`s named
`a`/`b`/`c`) all hash equal under kind-only, so the nearest-by-position tiebreak could silently
"recover" a pairing that looks unreordered even when it wasn't, breaking reorder detection itself.
Fixed with a two-tier match: exact `kind_and_value_hash` first (correct whenever the outer match
guarantees kind+value multiset equality), `kind_only_hash` fallback for whatever's left unpaired
(needed for `KindOnlyHash`-driven outer matches, where content values may legitimately differ).

New test (`solve_hash_descent.rs::reordered_commutative_container_is_distinguished_from_truly_
identical`) verifies a reordered Rust `use_list` gets `FullymappingSubtrees` while an untouched
sibling function does not.

**Result: 789 -> 787 mismatches vs. the 782 baseline.** `rust-next-font-imports-generator` improved
29 -> 27 but the fixture-level gap isn't fully closed: the benchmark's mismatch detection compares
`ASTMappingOperation`, not `ASTMappingReason`, and the human ground truth expects `MatchButNot
Identical` operation for these reordered pairs, not just a distinguishing reason - a cost-model
question (should a pure reorder cost more than 0, and should its operation differ from `Identical`?)
that's separate from what was asked and wasn't changed. Full test suite (341 tests, 5 ignored) green.

## Reordering cost + operation (added 2026-07-18)

Immediate follow-up user request: "make reordering cost more than 0 and change operation to
MatchButNotIdentical." The reordered container itself (`FullymappingSubtrees`) now gets
`ASTMappingOperation::MatchButNotIdentical` / `COST_UPDATE` instead of `Identical` / 0.

That alone still left every non-commutative *ancestor* of a reordered container (e.g.
`use_declaration`/`scoped_use_list` wrapping a reordered `use_list`) reporting `Identical`, since
they aren't commutative containers themselves and `pair_children_for_descent`'s reorder detection
only fires on the container that actually reordered. Fixed by collecting every reordered node's id
during the descent and, once a hash-descent root match's whole subtree is processed, walking each
one's ancestor chain (via `node_to_parent`) up to the match's own root, downgrading every
`Identical` ancestor along the way to `MatchButNotIdentical` - a container is never a true no-op
match if anything inside it, at any depth, wasn't. Ancestors keep their existing reason (only the
actual commutative container gets `FullymappingSubtrees`); only operation/cost change.

**Result: 787 -> 778 mismatches - now *below* the 782 baseline.** `rust-next-font-imports-generator`
flipped from a +5 regression to a **-4 improvement** (18 vs. the old pipeline's 22): the ancestor-
propagation logic catches cases `solve_commutative_structural_trees`'s single-level
`FullymappingSubtrees` reason never did (it never propagated non-identical status up through
non-commutative wrapper ancestors either). The full 86-fixture old-vs-new diff now shows **zero**
fixtures where the new pipeline is worse than the old one, and one where it's measurably better.
Full test suite (341 tests, 5 ignored) green.

The new pipeline is now at least as accurate as the old one on every fixture in the corpus. Next
step is the "final cleanup" above: flip the `use_new_pipeline` default, delete the ~9 superseded
modules/reasons/config fields, remove the toggle.

---


Requested by the user 2026-07-17: replace the current ~15-pass pipeline (see `Diff::from_code_
with_config` in `src/diff.rs`) with a smaller, six-phase one, as a **clean replacement** (delete
superseded modules/reasons/config fields directly - no side-by-side A/B period). All design
questions below were asked live and answered the same day; this is the resolved plan. Still open:
exact "fully resolved name" resolution scheme per kind/language, exact cost function for phase 4's
approximate-cost step, and phase 5's exact parameters (same as phase 3, or loosened?) - these are
implementation-time decisions, not blocking further planning.

## The six phases

1. **Hash-based, largest-subtree-first descent** - a generalized, reusable version of what
   `solve_identical_trees`/`solve_structurally_identical_trees` already do via `hash_tree_
   matching.rs`'s `HashMatchSpec`/`solve_with_node_list`: walk candidate nodes largest-subtree-
   first, and for each still-unmatched node, look up an after-side node sharing its hash, claim
   the match, then pair descendants positionally. The rework: make this genuinely reusable rather
   than the current `HashMatchSpec` (which reads a *named* field off `ASTMetadata` via a function
   pointer, e.g. `|m| &m.node_to_full_hash`) - the caller precomputes a node-id -> hash map with
   whichever hash algorithm it wants *before* invoking the engine, and passes that map plus an
   `ASTMappingReason` straight in as parameters. Called multiple times, once per hash algorithm:
   - `KindAndValueHash` (replaces `solve_identical_trees`/`IdenticalHash`)
   - `KindOnlyHash` (replaces `solve_structurally_identical_trees`/`StructurallyIdenticalSubtrees`,
     **and** `solve_multilevel_hash`'s 4 `Normalized*` variants - punctuation/literal/identifier-
     insensitive matching folds into this one coarser hash rather than 4 separate ones. Known,
     accepted tradeoff: this loses the 2 intermediate granularities `solve_multilevel_hash` had
     - e.g. "same structure and literals, renamed identifiers only" no longer gets its own
     matching tier, just falls to the same coarse kind-only bucket as everything else. Worth
     checking the accuracy impact via `benchmark_optimal_solutions` once implemented, even though
     this isn't gated behind a flag for A/B comparison per the "clean replacement" decision.)
   - A normalized-import-path variant (replaces `solve_import_nodes` - folded in as a hash variant
     rather than kept as its own phase, per the same "reusable hash descent" engine).
   `solve_commutative_structural_trees` is removed outright (see below - order-independence is now
   inherent to both primary hashes, not a bolted-on third one).

2. **"Move detection"** (originally named this in planning; renamed to **"contextual exact
   matching"** 2026-07-18, see below) - repurposed name/slot, **not** `solve_moved_subtrees.rs`
   (that module is deleted outright - the user confirmed "remove solve_moved_subtrees"). Phase 2
   instead houses `solve_comment_nodes` (comment-precedes-matched-node matching) and `solve_
   identical_diagnostic_statements` (identical logging/bail/assert/printf matching) - both are, in
   the user's framing, a form of "detecting where already-known content re-lands" rather than
   literal subtree-move recovery.

3. **Bottom-up expansion** - `solve_bottom_up_expansion.rs` as it exists today (Dice-coefficient
   vote-then-verify matching of containers via already-matched descendants), unchanged.

4. **Syntax-aware subtree matching** - a redesign of `solve_semantically_structural_nodes.rs`,
   generalized to also absorb `solve_greedy_anchor_blocks`'s job (confirmed: "phase 4 also handles
   anonymous containers") and `solve_large_flat_subtrees`'s job (confirmed: "a special case of the
   greedy solver" - once a pair is accepted and handed to `apted::for_nodes`, `resolve_forest`'s
   existing flat-tree fast path already routes to Myers automatically when applicable, so this
   likely needs no dedicated code of its own beyond making sure large flat containers are valid
   candidates in the same pool - not a separate pre-emptive phase).
   Candidates: reference nodes (`nodes::is_reference`'s per-language kind list - broader/coarser
   than the current pass's `nodes::is_semantically_structural`) matched by `(kind, fully-resolved
   name)` **when a name is resolvable**, falling back to positional anchoring (shared matched
   ancestor + kind-path, `solve_greedy_anchor_blocks`'s existing mechanism) for anonymous
   containers with no name at all (if-bodies, loop bodies, ...) - one unified greedy matcher
   covering both today's name-keyed pass and today's positionally-anchored one.
   "Fully resolved name" is new: today, `semantically_structural_nodes: HashMap<(String, String),
   usize>` stores at most *one* node per `(kind, name)` key and needs a separate impl/class-scoped
   pre-pass (Pass 1/Pass 0b in the current module) specifically because a bare method name like
   `new` collides across every `impl` in the file - "fully resolved" means encoding that scope
   into the name itself (e.g. `"Bar::new"` instead of bare `"new"`), letting one flat matching
   mechanism replace today's two-tier impl/class-then-everything-else split. **Confirmed: real
   N:M cases matter** (overloads, trait-impl duplicates), not just the common 1:1 case - the
   candidate-grouping data structure needs to hold a `Vec` per `(kind, name)` key, not a single id.
   Matching mechanism: for every candidate group, compute a **fast, approximate** cost for all
   valid before/after pairs in that group (same spirit as `solve_greedy_anchor_blocks`'s `sequence_
   edit_cost` - a direct-children-only estimate, not real tree edit distance), then greedily assign
   pairs whose cost clears a similarity threshold, cheapest first - mirroring `solve_greedy_anchor_
   blocks`'s `scored_pairs.sort_by(...)` + greedy claim loop, generalized from "compete within one
   positional-key group" to "compete within one (kind, name)-or-positional group." Once a pair is
   accepted, run **real APTED** on it (`apted::for_nodes`) to produce the actual mapping - same
   "pre-match via a cheap signal, diff for real" idiom every current heuristic pass already uses;
   `PostorderIndexer` already skips anything an earlier phase claimed, so nothing new needed there.

5. **Second bottom-up expansion** - phase 3 run again, after phase 4 has produced more matched
   descendants for it to vote on. Exact parameters (same as phase 3, or a looser Dice threshold
   the second time) TBD at implementation time.

6. **Final APTED** - `solve_final_apted`/`apted::for_roots` on the whole-file residual, unchanged.
   `solve_orphaned_semantic_nodes` (Pass 3, which currently runs *after* this) is deleted outright
   - confirmed: it's already a no-op today, kept only for documentation, so there's nothing to
   port forward.

## New hash algorithms: `KindAndValueHash` and `KindOnlyHash`

Replace `IdenticalHash` (today's full hash, `compute_full_hash`) and `StructurallyIdenticalSubtrees`
(today's structural hash, `compute_structural_hash`) - **and**, per the "fold it in too" decision
above, `solve_multilevel_hash`'s 4 normalized variants - with two algorithms of the same basic
shape as today's first two (kind+value = byte-identical subtree; kind-only = same shape, different
leaf values), but both gain a property neither current hash has cleanly: **order-independence
folded in per node kind**, not bolted on as a third, separate hash variant the way `compute_
commutative_structural_hash` is today. Concretely: consult `nodes::is_commutative_container(kind,
language)` (already exists, already per-language - enum variant lists, use-lists, struct field
lists, etc.) while hashing; if a node's own kind is commutative, hash its children's hashes
*unordered* (sort child hashes before combining) instead of in document order.

This directly fixes a bug already found and documented in `compute_commutative_structural_hash`'s
own doc comment (`code/hash.rs`, dated 2026-07-15, never applied): today, order-independence does
not propagate past the commutative container itself, because the non-commutative branch delegates
to plain `compute_structural_hash` instead of recursing back into itself - so a reordering-only
change still changes the hash of every *ancestor* of the reordered container (e.g. the `enum_item`
wrapping a reordered `enum_variant_list`), meaning `solve_commutative_structural_trees` (which
matches on the ancestor reference node, not the bare container) never actually fires for its own
documented use case. Building both new hashes to recurse into themselves unconditionally, checking
`is_commutative_container` at every level rather than only at the top, fixes this by construction.

`solve_commutative_structural_trees.rs` and its dedicated hash field/reason (`FullymappingSubtrees`)
are removed outright, since order-independence is now inherent to both primary hashes.

## Disposition of every currently-existing pass (resolved 2026-07-17)

| Pass | Fate |
|---|---|
| `solve_identical_trees` | replaced by phase 1 w/ `KindAndValueHash` |
| `solve_structurally_identical_trees` | replaced by phase 1 w/ `KindOnlyHash` |
| `solve_commutative_structural_trees` | **removed** - folded into both new hashes |
| `solve_multilevel_hash` | **removed** - folded into `KindOnlyHash` (accepted precision loss, see above) |
| `solve_import_nodes` | **removed** - folded into phase 1 as a hash variant |
| `solve_comment_nodes` | moved into phase 2 ("contextual exact matching"), unchanged internally |
| `solve_identical_diagnostic_statements` | moved into phase 2 ("contextual exact matching"), unchanged internally |
| `solve_moved_subtrees` | **removed** outright |
| `solve_bottom_up_expansion` | phases 3 and 5, unchanged internally |
| `solve_semantically_structural_nodes` | replaced by phase 4 (redesigned) |
| `solve_similar_flow_control` | folded into phase 4 - arm-overlap scoring becomes another candidate-grouping signal in the generalized greedy matcher, alongside name-based and positional anchoring |
| `solve_greedy_anchor_blocks` | **removed** - absorbed into phase 4 (anonymous-container handling) |
| `solve_large_flat_subtrees` | **removed** as a standalone phase - becomes an implicit special case inside phase 4 |
| `solve_orphaned_semantic_nodes` (Pass 3) | **removed** outright (dead code, already a no-op) |
| `solve_final_apted` | phase 6, unchanged |

Phase 4 is now the single largest consolidation point in the rework: it absorbs `solve_
semantically_structural_nodes` (name-based matching), `solve_greedy_anchor_blocks` (positional
matching for anonymous containers), `solve_large_flat_subtrees` (flat-subtree special case), and
`solve_similar_flow_control` (arm-overlap candidate grouping) into one generalized greedy matcher
with several different candidate-grouping signals feeding the same "approximate cost -> greedy
assign -> real APTED" mechanism.

*  IMPLEMENTED (2026-07-15): Per-pass ablation study infrastructure (`HeuristicConfig` in
   `src/diff.rs`, `Diff::from_code_with_config`/`diff_code_with_config`, 14 `--solver-X`/
   `--no-solver-X` flag pairs on `benchmark_optimal_solutions`, `ablation_study.sh`). Every
   heuristic/algorithm pass in the pipeline can now be independently toggled without touching any
   of the 20+ existing zero-config call sites (default config is unchanged unless noted below).
   A leave-one-out sweep over the 86-fixture `optimal_solutions` corpus found:
     - 3 passes net-negative individually (disabling them *improved* the aggregate mismatch
       count): `solve_import_nodes` (-89), `solve_similar_flow_control` (-82),
       `solve_bottom_up_expansion` (-69).
     - 5 passes with zero measured effect individually: `solve_structurally_identical_trees`,
       `solve_commutative_structural_trees`, `solve_multilevel_hash`, `solve_greedy_anchor_blocks`,
       `solve_orphaned_semantic_nodes`.
     - `solve_final_apted` and `solve_identical_trees` are load-bearing (+14873 and +1126
       respectively when disabled) - never disable either.
   **Default changed**: `HeuristicConfig::default()` now disables the 3 net-negative passes only
   (`solver_import_nodes`, `solver_similar_flow_control`, `solver_bottom_up_expansion` = `false`).
   Result: 1022 -> 782 mismatches (-23%) on the benchmark corpus, runtime 309s -> 368s (+19%,
   less pruning work for `final_pass` to do). Only 1/86 fixtures regressed
   (`cpp-ladybird-refactor-variables-if-changes`, 62 -> 109 mismatches); 4 improved.
   **First attempt disabled all 8 passes (3 net-negative + 5 zero-effect) - reverted in favor of
   the narrower 3-only default**: 1022 -> 815 (-20%) but runtime 309s -> 545s (+76%) and 2/86
   fixtures regressed. The "zero individual effect" passes turned out not to be free to disable:
   they were quietly pruning work off the expensive `final_apted` fallback even when they weren't
   winning any matches outright, so removing them alongside the net-negative 3 made both accuracy
   and runtime worse than removing just the 3. Lesson: leave-one-out ablation deltas don't compose
   additively - always re-measure the combined config, not just sum the individual deltas.
   **`src/test/optimal_solutions/*.rs` limits reclamped to the new baseline** (2026-07-15): every
   `assert_matches_human_mapping_within_limit(name, N)` call's `N`, and every plain
   `assert_matches_human_mapping(name)` whose fixture no longer matches exactly, was regenerated
   from a fresh `benchmark_optimal_solutions --csv` run against the new default and updated to the
   current mismatch count (10 files touched: `c_cpython_autogenerated_code` 127->58,
   `c_postgres_real_logic_change` 17->8, `cpp_ladybird_refactor_variables_if_changes` 62->109 (the
   one fixture the new default regressed), `rust_turbopack_module_rule` 175->52, plus 6 fixtures
   that turned out to already be silently exceeding their old implicit 0-limit before this change
   even happened - `cpp_laydbird_change_function_signature`, `cpp_tensorflow_switch_to_primitive_types`,
   `csharp_sonarr_change_type`, `kotlin_nextcloud_change_function_fingerprint`,
   `kotlin_refactor_function`, `rust_sniffnet_protocol` - now converted to `_within_limit` at their
   actual counts). Full suite is green again (336 passed, 0 failed, 5 ignored). Full per-flag
   numbers and CSVs: `research/ablation/` (from `./ablation_study.sh`).

*  IMPLEMENTED (2026-07-14): Import path normalization and matching (`ASTMappingReason::NormalizedImportPath`,
   `src/diff/solve_import_nodes.rs`). Normalizes import paths by removing surrounding quotes,
   normalizing path separators, handling relative import prefixes, and matching imports by normalized
   path rather than syntax. Wired into the pipeline after multi-level hash matching and before
   comment node matching. This allows the algorithm to recognize that imports with different formatting
   (e.g., `use "std::path";` vs `use 'std::path';`) but the same path are actually the same.
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
   TRIED AND REVERTED (2026-07-15), the leaf-level variant flagged above as worth trying: graduated
   `UnitCostModel::ren`'s same-kind-different-text *leaf* cost (identifiers/generic tokens) by
   `nodes::leaf_texts_similar`'s underlying character-bigram Dice ratio (exposed as
   `nodes::leaf_text_dice_ratio`), instead of the flat `COST_UPDATE` every such pair paid before.
   Motivated by a real, confirmed gap: `kotlin-nextcloud-change-function-fingerprint` and
   `kotlin-refactor-function` both exhibit a same-kind-leaf multi-candidate tie under
   `reason APTED("final_pass")` (raw DP cost, not a named heuristic) when a parameter is inserted
   mid-signature and every later parameter shifts by one slot.
   **Headroom problem, found before implementing:** flat unit costs (`COST_UPDATE = 1`,
   `COST_DELETE + COST_INSERT = 2`) leave no room to grade between "always rename" and "ties
   replace" - a naive 2-tier integer split ties the cheap tier with outright delete+insert and
   flips clear renames like `fetch_user` -> `fetch_user_data` (Dice ~0.78) into the penalized tier.
   Fixed by giving `UnitCostModel::del`/`ins`/`ren` their own internal `REN_SCALE` (x100), used only
   inside those three methods - APTED's search only ever compares costs relatively, so the absolute
   scale is free, and this bought room to grade leaf-rename cost within `(LEAF_RENAME_MIN_COST,
   LEAF_RENAME_MAX_COST)` while staying strictly below the rescaled `del()+ins()`.
   **Two latent leaks the rescale surfaced, both fixed before benchmarking (still relevant if anyone
   revisits internal cost rescaling here):** (1) `FORBIDDEN_RENAME_COST`/`ren`'s different-kinds
   branch had been hardcoded from the raw, un-rescaled `COST_DELETE + COST_INSERT + 1` - left as-is,
   the containment veto (`ContainmentCtx::adjust`) would have gone inert (nearly every rescaled cost
   now exceeds the stale sentinel) or, worse, inverted into the DP's *preferred* option. (2)
   `add_prune_mappings`'s `subtree_del_cost`/`subtree_ins_cost` and `classify_match`'s
   disallowed-cross-kind branch called `cost_model.del/ins/ren` directly to populate *reported*
   `ASTMapping.cost` - not just APTED's internal search - so the rescale leaked a 100x inflation
   into real mapping costs (`cargo test`'s `test_hello_world_added_message` et al. went from
   asserting `cost == 12` to actually getting `1200`). Fixed by pointing those reporting call sites
   at the flat `COST_DELETE`/`COST_INSERT`/`COST_UPDATE` constants directly, decoupled from
   `UnitCostModel`'s internal search scale - the same split `cost.rs::operation_cost` and
   `classify_match`'s leaf-update branch already had, just extended to the two sites that had been
   silently sharing the search-time model instead.
   **Result:** the two motivating fixtures moved by exactly **zero** mismatches each (31->31,
   64->64) - the graduation never engaged for them, because the human-correct pairs in both
   (`capability`->`capability`, `showTaskActions`->`showTaskActions`) are text-*identical*, so `ren`
   was already returning 0 under the old flat model too; the actual gap is elsewhere in how the
   surrounding shifted structure gets scored, not in leaf-rename cost. Across the full 86-fixture
   corpus: 5 regressed (`rust-firefox-webrenderer-borders` +8, `go-user-slices-library` +6,
   `cpp-optimize-algorithm` 0->5, `rust-zed-workspace-tasks` +3,
   `cpp-laydbird-change-function-signature` +1) against 2 improved (`c-nginx-add-typedef` -15,
   `cpp-ladybird-refactor-variables-if-changes` -2) - net **+6** mismatches, and
   `cpp-optimize-algorithm` went from a *previously-perfect* 0-mismatch fixture to 5. Checked via
   `--details`: real content mismatches (a `return_statement` deleted wholesale despite an identical
   counterpart existing; an `identifier` cross-matched to an unrelated `field_identifier`), not
   punctuation ties - same failure signature as the container-level attempt above (a previously-good
   tie-break gets upended by a signal that's live but mistargeted). Also measurably slower: the full
   corpus benchmark went from under 2 minutes to 5.6+ minutes in `--release`, since every same-kind
   different-text leaf comparison now does a bigram-hashset computation instead of a constant
   lookup, on a hot path (`ren` is called extremely often during APTED's search).
   Reverted in full (`nodes::leaf_text_dice_ratio` extraction, `UnitCostModel`'s `REN_SCALE`/graded
   leaf branch, the reporting-path decoupling in `subtree_del_cost`/`subtree_ins_cost`/
   `classify_match` - the last of these was only needed *because* of the rescale, so it reverts too
   rather than being kept as drive-by cleanup). Whoever picks this up next: the mechanism itself
   works exactly as designed (it's what produced `c-nginx-add-typedef`'s -15 and
   `cpp-optimize-algorithm`'s regression alike) - the premise that failed is that leaf-rename cost
   was the right place to look for the `kotlin-nextcloud-change-function-fingerprint`-style gap.
   That gap needs a signal sensitive to the *shifted-position* structure, not leaf text similarity,
   since the correct leaf pairs there were already free matches. Separately, `c-nginx-add-typedef`'s
   -15 is a real, unexplained win worth investigating on its own before reusing this mechanism -
   just not sufficient by itself to justify the net regression and perf cost of shipping it broadly.

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

*  FIXED (2026-07-14): Premature/irreversible pruning in `solve_semantically_structural_nodes`'s
   Pass 3. **Fix:** Moved `solve_orphaned_semantic_nodes` to run AFTER the final full-tree APTED
   pass in `Diff::from_code`, and made it a no-op (the final APTED pass already handles all
   possible structural matches). Previously, when a name-keyed anchor (`impl_item`/`function_item`)
   failed to find a counterpart, Pass 3 would immediately mark the whole subtree deleted/inserted
   via `apted::for_nodes` with empty opposite forests, before the final full-tree APTED pass had
   a chance. Surfaced by the `rust-turbopack-module-rule` optimal_solutions test where
   `impl ModuleType` was renamed to `impl ConfiguredModuleType` - the type name changed but the
   body had structural similarities that APTED could match. Now APTED runs first and finds these
   matches. Tradeoff: increases mismatch count with human solution from <=169 to 172 for
   rust-turbopack-module-rule (limit increased to 175), but represents more accurate structural
   matching for syntax-only diffing.

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

*  `cpp-ladybird-refactor-variables-if-changes` (optimal_solutions test, investigated 2026-07-16,
   no code changed): same class of gap as `rust-algorithm-change` above, not a bug. Before has
   `auto& svg_graphics_element = as<SVG::SVGGraphicsElement>(*dom_node); auto active_view_box =
   svg_graphics_element.active_view_box();`; after replaces this with an `if`/`else if` chain
   (`if (auto* svg_graphics_element = as_if<SVG::SVGGraphicsElement>(*dom_node)) active_view_box =
   svg_graphics_element->active_view_box(); else if (...) ...`). The human mapping deletes the two
   old declarations wholesale and inserts the new `if`/`else if` chain wholesale. codediff instead
   reuses the shared type name/call shape/`*dom_node` argument, matching the old assignment into
   the new `if` condition - cheaper under unit cost, so this is the DP finding the objectively
   lower-cost mapping, not losing a tie. Confirmed two ways: (1) `algorithm_cost` (660) <
   `human_cost` (711) in the benchmark CSV - codediff's mapping is provably cheaper, so a cost
   function that better approximates true edit distance moves *away* from the human mapping here,
   not toward it; (2) `--no-solver-greedy-anchor-blocks --details` on this fixture still gives 109
   mismatches unchanged - the plain final-APTED pass independently finds the same reuse under the
   same unit-cost model, so this isn't a `solve_greedy_anchor_blocks` candidate-selection bug either.
   The actual lever would be `UnitCostModel::ren`/`del`/`ins` in `apted/common.rs` (the search-time
   model `for_nodes` actually uses), not `cost.rs` (post-hoc reporting only, never consulted during
   matching). Already-tried, already-reverted cost-model levers in this exact space: the leaf-text-
   Dice-graduation attempt above and the container-dissimilarity-cost attempt (see git history/prior
   TODO revisions) - both failed for the same reason ("text similarity can't distinguish
   near-duplicate from same-entity-edited"), and neither would even engage here since the reused
   text genuinely *is* near-identical (`SVG::SVGGraphicsElement`, `*dom_node`, `active_view_box` all
   literally recur). The one untested, structurally different lever: a relocation penalty for reuse
   across a changed control-flow-kind ancestor path (assignment -> if/else chain) - deliberately
   sacrifices edit-distance minimality for human-diff-readability rather than better approximating
   it, so it needs its own careful validation the same way (b) above does, not a quick tweak.
   Decision: dropped, not pursued - single fixture, and codediff's mapping (reusing `as<T>(x)` ->
   `as_if<T>(x)`) is arguably a defensible diff on its own merits, not obviously worse for review
   than the human's wholesale rewrite framing.
