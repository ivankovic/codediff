# HANDOVER: Implementation guide for the top-5 diff-quality ideas

This is the companion to the "Top 5 quality ideas" section in `TODO.md`. That file says *what*
and *why*; this one says *how*, in enough detail that a fresh session (human or agent) can start
implementing without re-deriving the architecture or re-running failed experiments.

Written 2026-07-06, against the baseline: **270 total mismatches** from
`cargo run --release --bin benchmark_optimal_solutions`.

---

## How to measure anything

```
cargo run --release --bin benchmark_optimal_solutions   # ~10s, prints per-fixture mismatch table + total
cargo test optimal_solutions                            # ~75s debug; same checks as pass/fail tests
cargo test --release                                    # full suite, ~15s; includes the oracle tests
```

The benchmark binary lives in `src/bin/benchmark_optimal_solutions.rs` and is a thin loop over
`codediff::test::helper::human_mapping::compute_mismatches(name)` for every fixture directory in
`src/test/data/diffs/` that has a `human_mapping.json`. One mismatch = one human-mapping entry
(a node or node pair) whose expected mapping codediff didn't produce.

**Judge every change by the TOTAL, not by one fixture.** Every experiment so far that fixed the
target fixture in isolation regressed others. A change is good iff total goes down and no
currently-0 fixture becomes non-0.

**Recommended first task (30 min): add a `--details <fixture>` flag to the benchmark binary** that
prints, for each mismatch, what codediff actually mapped the node to *and the `ASTMappingReason`*
of that mapping (via `diff_ast.mapping_for_node(&node_id)`). The reason field tells you which pass
made the wrong call (`IdenticalHash`, `StructurallyIdenticalSubtrees`, `APTED`, ...), which is the
single most useful diagnostic and currently requires manual debugging to get. All five ideas below
start with "diagnose which pass produced the bad mapping" - build the tool once.

New ground truth is authored with the `human_solver` TUI binary (`src/bin/human_solver.rs`); 14
fixtures still lack a `human_mapping.json` (shown as "unsolved" in the benchmark). The user runs
human_solver himself - don't regenerate mappings without asking.

## Architecture crash course

The pipeline is `Diff::from_code` in `src/diff.rs` (~line 181). Passes run in order; each pass
writes into a shared `ASTDiff` and **later passes skip anything already mapped** - mappings are
first-writer-wins and never revisited:

1. `solve_identical_trees` - full-hash matching of reference nodes, largest first.
2. `solve_structurally_identical_trees` - structural-hash (kinds only) matching.
3. `solve_semantically_structural_nodes` - name-keyed anchoring (fn/impl/class by identifier).
4. `solve_similar_flow_control` - Jaccard-scored match/switch arm pairing (threshold 0.75).
5. `solve_identical_diagnostic_statements` - byte-identical log/bail/panic statements.
6. `solve_semantically_structural_nodes::solve_orphaned_semantic_nodes` ("Pass 3") - anything
   still orphaned gets **irrevocably** blanket-marked delete/insert. This is the known
   rust-turbopack killer (see TODO.md "Known gaps").
7. `apted::for_roots` (`Algorithm::ZhangShasha`) - global optimal tree edit distance over the
   residual forest.

Step 7 internals (`src/diff/apted/common.rs::resolve_forest`), where most work happens:

- `PostorderIndexer::build` **prunes already-mapped nodes out of the forest entirely**.
- `ContainmentCtx` forbids renames that contradict where pruned descendants landed
  (via `FORBIDDEN_RENAME_COST = COST_DELETE + COST_INSERT + 1`).
- `compute_delta_zhang_shasha` + `compute_edit_mapping` produce a `Vec<RawDecision>`
  (`Match(b,a)` / `Delete(b)` / `Insert(a)`) - the globally cost-optimal mapping.
- Decisions land in `before_decision: HashMap<usize, BeforeDecision>` /
  `after_decision: HashMap<usize, AfterDecision>`.
- `compute_has_match_below` computes, per node, whether anything beneath it is matched.
- **Emission**: `emit_before_subtree` / `emit_after_subtree` / `emit_match` translate decisions
  into `ASTDiff` mappings. Emission already handles the "deleted node whose children are reused"
  shape: node costs 1, children recurse independently.

Cost model (`UnitCostModel::ren`): same-kind internal pair = **0** (this is why unrelated
same-kind containers get "reused" - it's always ≥2 cheaper than delete+insert); same-kind leaf =
0 if same text else `COST_UPDATE`; cross-kind = `COST_UPDATE` if `kinds_update_allowed` (operator
families in `src/diff/nodes.rs`) else `FORBIDDEN_RENAME_COST`.

Key data (`src/code.rs::ASTMetadata`): `node_info` (kind/text/children per node id),
`node_to_full_hash` + `full_hash_to_node` (content hash, reverse map is a set),
`node_to_structural_hash` + reverse (kinds-only hash), `node_to_subtree_size`,
`reference_nodes_ordered` (human-scale anchor nodes, largest first).

`ASTDiff`: `mapping: HashMap<(before_id, after_id), ASTMapping>` plus `before_node_map` /
`after_node_map` (id → partner id, 0 = deleted/inserted). **There is no remove API** - if your
pass needs to overturn an existing delete/insert (ideas 2 and 3), you must add one; see the
pitfalls in idea 2.

## Hard-won constraints - do not relearn these

1. **Never change `UnitCostModel::ren`'s returned costs.** The `assert_distance_matches_oracle*`
   tests in `common.rs` pin the APTED engine against Zhang-Shasha byte-for-byte on total cost, and
   the DP's optimality argument depends on the cost model. The sanctioned pattern (used by the
   generic-token gate, shipped 2026-07-06) is an **emission-time override**: let the DP decide,
   then in `emit_match` demote decisions you don't like into delete+insert. `emit_match` is the
   single funnel every fresh `Match` decision flows through.

2. **Symmetric dice-coefficient demotion of container matches was tried and lost** (47/8 passing
   tests → 27/28; fully reverted). Root cause: dice punishes pure-addition edits - appending new
   code under an unchanged container dilutes the ratio. Any support-score must treat "one side
   fully matched" as full support (see idea 4).

3. **Cascading demotion (demoting a container match together with all descendant matches) was
   tried and lost.** A spurious wrapper match often has a perfectly good match nested inside it
   (e.g. an unchanged callback body inside a new `try`). Demote the single node only; emission
   handles "deleted node, children reused" natively via `before_has_match_below`.

4. **Hash-based pre-matching of arbitrary interior nodes was tried and reverted** (predates this
   session; see the comment in `resolve_forest`'s fast path). Same-kind interior nodes rename for
   free, so a hash "anchor" can hand genuinely-new content a free skeleton. Interior-node
   anchoring needs containment/context checks, not just hash equality.

5. **The immediate-parent-only context check is too strict.** The generic-token gate originally
   required the leaf's direct parent to be matched and regressed 3 fixtures, because one inserted
   wrapper level (e.g. `identifier` → `reference_declarator > identifier`) breaks it. Bounded
   ancestor climb (`MAX_CONTEXT_ANCESTOR_DEPTH = 2`) fixed all three. Reuse
   `has_nearby_matched_ancestor` rather than writing a new parent check.

6. **`ASTDiff.is_valid` / `is_complete`** (`src/diff.rs`) are asserted by tests: every mapped pair
   must be same-kind or `kinds_update_allowed`, and every node in both trees must appear in some
   mapping. Any pass that re-maps nodes must leave both invariants holding.

---

## Idea 1: Wrapper insert/unwrap detection

**Target fixtures**: javascript-fix-promises (5), kotlin-add-data-class (4), parts of
rust-turbopack-module-rule (207) and python-api-change (10).

**The key realization**: emission *already supports* the wrapper shape (inserted node costs 1,
children independently matched), and plain tree edit distance *can* express it (delete/insert a
node splices its children to the parent). So when a wrapper edit round-trips as bulk
delete+insert, the wrapped content's match was lost **upstream** - not at emission. Diagnose
before coding:

1. Run `--details` (see above) on javascript-fix-promises and kotlin-add-data-class. For each
   wrongly-deleted node, note the reason on the mapping that *should* have been a match.
2. Two likely culprits, with different fixes:
   - **An earlier heuristic pass blanket-decided the region** (reason will be a non-APTED one, or
     the region was orphaned by Pass 3). Fix belongs in that pass, or in idea 2's recovery pass.
   - **The DP chose an alternative equal-or-cheaper mapping** (reason `APTED`): reusing some
     same-kind skeleton for 0 instead of paying 1 per wrapper node. This is a genuine tie or
     near-tie the cost model can't see. Fix: a *tie-break at emission* is not possible (the
     decision is already made), so this sub-case needs idea 3's slot-aware re-matching or idea 4's
     demotion of the competing spurious match - implement those first and re-measure; the wrapper
     case may fall out for free.
3. Only if a real gap remains: add a post-decision, pre-emission recovery in `resolve_forest`
   (same insertion point as the reverted `demote_unanchored_matches`, i.e. right after the
   `RawDecision` loop fills `before_decision`/`after_decision`): for each `Delete(b)` where `b`'s
   full hash equals the full hash of some `Insert(a)` **and** `a` sits under `b`'s parent's match
   target (containment check via parent maps - `build_parent_map` exists), flip both to
   `Match(b, a)` and mark all descendants matched pairwise (hashes equal ⇒ shapes identical;
   mirror `emit_identical_subtree`'s lockstep walk).

**Pitfalls**: don't claim the same `a` twice (process largest-first, keep a claimed set); respect
`ContainmentCtx`-style ordering (only re-match within the corresponding region, or you recreate
the free-skeleton bug from constraint 4).

**Validation**: javascript-fix-promises should drop 5→~0 (the `try` wrapper), kotlin-add-data-class
4→~0. Watch typescript-add-error-handling (currently 0, also a try-wrapper fixture - it works
today and must keep working).

## Idea 2: Move-detection recovery pass over unmatched islands

**Target**: rust-turbopack-module-rule (207 - the dominant term in the total).

**Design**: a new final pass in `Diff::from_code`, after `apted::for_roots`. At that point the
diff is complete; this pass upgrades delete+insert pairs into matches.

1. Collect all before-nodes mapped to 0 and all after-nodes mapped to 0 (walk
   `diff.before_node_map` / `after_node_map`).
2. Index the deleted side by full hash, but **only distinctive subtrees**: require
   `node_to_subtree_size >= N` (start N=4, tune) *or* the hash to be rare
   (`full_hash_to_node[h].len() <= 3` on both sides) - otherwise you're back to matching stray
   `;` tokens, the exact disease this project just cured.
3. Iterate deleted candidates largest-first (sort by `node_to_subtree_size`). For each, find an
   inserted node with the same full hash. Take the first unclaimed one; claim both subtrees.
4. Re-map the pair: root gets `ASTMappingOperation::Move` (exists, `COST_MOVE = 0`) with a new
   `ASTMappingReason::MovedSubtree`; descendants get pairwise matches via a lockstep walk (hash
   equality guarantees identical shape).
5. **You must remove the old `(b, 0)` and `(0, a)` entries** for every node in both subtrees.
   `ASTDiff` has no removal API today. Add `ASTDiff::remove_mapping(before_id, after_id)` that
   erases from all three maps - and be careful with the reverse maps: `before_node_map[0]` /
   `after_node_map[0]` are garbage slots shared by every delete/insert (each `add_mapping(b, 0, ..)`
   overwrites `after_node_map[0]`), so only remove the *keyed* direction for null mappings, never
   trust the 0-side reverse entry.

**Extension for turbopack specifically**: exact-hash moves won't catch renamed content
(`ModuleType` → `ConfiguredModuleType` renames identifiers inside). After exact-hash recovery,
optionally do a second round on *structural* hashes (`node_to_structural_hash`) with a higher size
floor (N≥10) - structurally identical big subtrees whose leaf texts differ are "moved + renamed".
Measure the exact-hash round first; it may already collapse most of the 207.

**Validation**: turbopack should drop massively; nothing else should move. The rest of the suite
exercises no moves, so any other fixture changing means your distinctiveness floor is too low.

## Idea 3: Slot-aware same-kind container matching

**Target**: cpp-optimize-algorithm (26), javascript-add-array-method (14).

**The human rule being encoded**: if a statement was deleted and a same-kind statement was
inserted *in the same slot* (same matched parent, corresponding sibling position), humans read it
as "that statement, edited" (`MatchButNotIdentical`), with matching structural tokens (`return`,
`;`, `{`, `}`) and delete+insert for the differing content.

**Design**: post-decision, pre-emission pass in `resolve_forest` (same hook point as idea 1 step
3), or a final pass over `ASTDiff` like idea 2 - the former is easier because decisions are still
mutable there and emission does the bookkeeping for you.

1. For every matched pair `(P_b, P_a)` (from `before_decision` + pre-existing anchors), collect
   `P_b`'s children decided `Delete` and `P_a`'s children decided `Insert`, in sibling order.
2. Run an LCS over the two lists keyed by node *kind*. Each LCS pair `(b, a)` of same-kind
   children is a slot correspondence: flip `Delete(b)`+`Insert(a)` into `Match(b, a)`.
3. Recurse into each newly-matched pair: their children repeat the same alignment. This is
   Chawathe-style alignment, and it naturally does the right thing on cpp-optimize-algorithm:
   `return_statement ↔ return_statement` match, `return ↔ return` and `; ↔ ;` match (same kind,
   same slot), while `identifier` (`min`) vs `pointer_expression` (`*std::min_element(...)`) are
   different kinds, stay delete+insert.
4. Do **not** flip pairs where both subtrees are large and share nothing - add a cheap sanity
   guard: skip if both subtree sizes > K (say 20) and they share zero common descendant hashes.
   Otherwise a fully-rewritten function body gets force-matched statement-by-statement, which is
   exactly the `rust-algorithm-change` trap documented in TODO.md (the human there wants a
   *wholesale replacement*, not statement reuse - that fixture is currently green; keep it green).

**Interaction with the generic-token gate**: newly-matched structural tokens will pass the gate
automatically (their parent is now matched), so no changes needed in `emit_match`.

**Validation**: cpp-optimize-algorithm 26→low single digits, javascript-add-array-method 14→low.
Watch rust-algorithm-change and python-refactoring (both currently 0, both contain rewritten
loop bodies).

## Idea 4: Asymmetric (recall-based) container-support validation

**Target**: residual "skeleton reuse" matches that ideas 1-3 don't remove; historically
cpp-optimize-algorithm and python-api-change shapes.

**This is the corrected re-run of the reverted experiment.** Differences from the version that
lost, in order of importance:

1. **Recall, not dice**: support = `max(matched_desc / size(b), matched_desc / size(a))` where
   `matched_desc` counts descendants of `b` whose match target lies inside `a`'s subtree
   (both fresh `Match` decisions and pre-existing anchors - `ContainmentCtx`'s
   `before_pruned_targets` already computes the pre-existing side). Pure additions score ~1.0 on
   the before side and survive.
2. **No cascade**: demote only the container node itself (`Delete(b)` + `Insert(a)`); descendant
   matches stay and emission renders them correctly.
3. **Bottom-up processing** so a parent's `matched_desc` doesn't count matches you already
   demoted below it.
4. **Exemptions before scoring**: same full hash (identical twins are always legit), or any
   pre-DP anchor inside (an earlier pass vouched for this region), or leaf nodes (no descendants;
   they're idea 5's problem).

**Where**: exactly the old insertion point - `resolve_forest`, after the decision loop, before
`compute_has_match_below` (which must see post-demotion decisions).

**Threshold**: start at 0.3-0.4, *not* GumTree's 0.5 - the DP's mappings are already optimal, so
this pass should only catch egregious skeleton reuse, not adjudicate close calls. Sweep 0.2-0.5
against the benchmark total and pick the minimum.

**Validation watchlist** (the fixtures the failed experiment broke): typescript-add-generics,
cpp-add-const-correctness, kotlin-add-validation, java-add-logging, rust-add-to-existing-use,
c-ffmpeg-added-typedef-to-enum. All currently 0; all must stay 0.

## Idea 5: Text-similarity gating for leaf Updates

**Target**: scattered single-node mismatches (python-api-change, javascript fixtures); cheapest
of the five, good warm-up.

**Where**: extend `demote_unsupported_generic_token_match` in `common.rs::emit_match` - it
already intercepts every fresh leaf-pair decision. Current logic: demote if either kind is a
generic token and no nearby matched ancestor. Add: if the pair is same-kind, both leaves,
**different text** (i.e. would be classified `Update`), require
`has_nearby_matched_ancestor(before_id, ctx, diff) || text_similar(t1, t2)`.

**`text_similar`**: normalized similarity ≥ ~0.6. Use character-bigram dice (simple, no deps,
symmetric, cheap: build two `HashSet<[u8;2]>`); guard the degenerate cases - texts of length ≤ 2
have no/few bigrams, so short identifiers (`i` → `j`) and small numbers (`0` → `1`) always fail
the text test and are saved only by the context arm of the OR. That's correct behavior: a
same-slot `i`→`j` rename has a matched parent; a cross-function `i`→`numbers` doesn't.

**Pitfalls**:
- Don't touch identical-text leaf pairs in this idea (they're `Identical`, cost 0). Gating
  identical-text *identifier* reuse across unrelated code is a plausible follow-up but is a
  separate, riskier change - measure it separately if at all.
- The demotion emits `add_delete_mappings(b) + add_insert_mappings(a)` - both are subtree-safe
  for leaves and preserve `is_complete`.
- Keep the generic-token branch first; operator kinds must not accidentally take the
  text-similarity path (`<` vs `<=` are 1-char texts anyway).

**Validation**: expect small wins (2-6 total). If the total moves the wrong way, the threshold
is eating legitimate renames - check python-api-change's `v1`→`v2` URL-ish updates specifically.

---

## Suggested order

1. **Tooling first**: `--details` flag on the benchmark binary (prerequisite for everything).
2. **Idea 5** (half a day, low risk, exercises the emission-override pattern end-to-end).
3. **Idea 3** (the LCS alignment pass; unlocks cpp-optimize-algorithm + javascript-add-array-method,
   and its slot machinery is reusable by idea 1).
4. **Idea 2** (move recovery; biggest absolute win via turbopack, independent of the others).
5. **Idea 1** (re-diagnose after 2+3 landed - much of it may already be fixed; implement only the
   residual).
6. **Idea 4** last (highest regression risk, needs the watchlist green before starting so its own
   effect is unconfounded).

After each idea lands: run the benchmark, record the new total in TODO.md next to the idea, and
run `cargo test --release` - the oracle tests and the currently-green optimal_solutions tests are
the regression fence.
